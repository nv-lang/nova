<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 153 (umbrella) — Production-grade `Vec[T]` / `[]T`: API-паритет, итераторы, слайсы

> **Создан:** 2026-06-13.  **Статус:** 🟡 **IN PROGRESS** — **153.0 ✅ ЗАКРЫТ** (2026-06-13,
> branch `plan-153`, commit `2a5df8e4`; см. «Статус 153.0» ниже); **153.1 🟡 ЧАСТИЧНО**
> (core API + fluent ✅, консолидация отложена); **153.2 ✅ ЗАКРЫТ (Phase A)** (ленивые
> итераторы, `plan-153.2-mono-closures`, commits `996ca01a`+`caf56226`, D260); **153.3
> ✅ ЗАКРЫТ** (sort/search); **153.4 ✅ ЗАКРЫТ ЦЕЛИКОМ** (A: eager slices `5ccccf72`; B: lazy
> chunks/windows `37884153`, D262 IMPLEMENTED ЦЕЛИКОМ);
> **153.5 ✅ ЗАКРЫТ** (restructure-ops + flatten/вложенные ресиверы, commits `e8f700e4`+
> `1c323d0e`+`16753d23`, D263); **153.6 ✅ ЗАКРЫТ** (Hash ✅ + FromIterator/collect-target
> ✅ D264; HashMap-Vec-key отложен). P1.
> **Цель:** коллекция `Vec[T]` Nova не хуже (а где можно — лучше) Go / Rust / TS / Kotlin /
> Java — по полноте API, итераторам, слайсам и предсказуемости стоимости. `[]T` —
> **чистый алиас** `Vec[T]` (D239).
> **Эстимат (весь umbrella):** ~14–20 dev-day, декомпозирован на 153.0–153.6.
> **Model:** Sonnet 4.6 + High + Thinking ON (153.2 итераторы — Opus).
> **Зависит от:** Plan 131 (`Vec` на RawMem), Plan 138 (D238 `Index`/D240 `MutIndex`/
> D239 `[]T≡Vec`), Plan 90.1 (extend-family), Plan 96 (sub-slice views),
> D58/D241/D242 (`Next`/`Iter`), Plan 137 (`Compare`/`Equal`/`Hash`/`Clone`).
> **Coordinate:** **Plan 140.2** (Vec `@index` bounds-as-contract) — НЕ дублировать,
> ссылаться. **Plan 152** (str — байтовая линза = `ro []u8` = `Vec[u8]`-view).
> **Предполагает примитивы:** скалярные `int.min(b)`/`int.max(b)` (метод-форма
> `a.max(b)`, как `a.sin()`) — ✅ **реализовано** (`[M-153-scalar-min-max]` CLOSED 2026-06-16, `std/runtime/defaults.nv`, все числовые типы).
> нужны для shrink-to-min-идиомы и `@min`/`@max`-терминаторов итератора (153.2).
> Если не добавлены к старту — добавить как мелкий number-примитив (Ф.0).
> **Предложено пользователем:** «привести Vec в порядок; `[]T` = алиас на `Vec[T]`».

---

## 0. Проблема и принцип

`Vec[T]` реализован на Nova поверх `RawMem` (Plan 131): `{data *mut T, len, cap}`.
`[]T` — синтаксический псевдоним `Vec[T]` (D239). Текущее состояние:

- **Расщепление модуля:** ядро в [vec_owned.nv](../../std/collections/vec_owned.nv)
  (`Vec[T]`-методы), а eager-комбинаторы (`map`/`filter`/`fold`/`any`/`all`) — в
  отдельном [vec.nv](../../std/collections/vec.nv) на `[]T`. Дубль концепций,
  непоследовательное размещение.
- **Нет ленивых итераторов:** `[]T.map(f) -> []U` материализует на каждом шаге
  (O(n) промежуточных аллокаций на цепочке); `VecIter` умеет только `next()`. Нет
  `iter().map().filter().collect()` (Rust)/Sequence (Kotlin)/Stream (Java).
- **Прод-пробелы:** нет `sort`/`binary_search`/`contains`/`index_of`/`dedup`/
  `chunks`/`windows`/`zip`/`enumerate`/`take`/`skip`/`min`/`max`/`sum`/`drain`/
  `rotate`/`split_at`/`concat`/`flatten`/`resize`/`swap`; нет `@hash`
  (нельзя класть `Vec` в `HashSet`/ключом).
- **Слайсы — модель уже верна (Plan 96), но surface неполон:** `v[a..b]` **уже**
  возвращает zero-copy view **того же типа** `[]T`=`Vec[T]` (`Self{data:@data+start,
  len:n, cap:n}`, cap==len → push реаллоцирует → silent detach; D238 + Plan 96
  **D-single-type**, «НЕ Rust-стиль 2 типов»). Не хватает только `split_at`/`chunks`/
  `windows`/`as_slice` поверх той же модели.

**Принцип (Rust Vec/slice + cost-transparency, как в Plan 152):**

> **`Vec[T]` — владеющий растущий буфер. `[]T` — его чистый алиас. Доступ/итерация/
> срез предсказуемы по стоимости (этос D135): индексация O(1), ленивые итераторы без
> промежуточных аллокаций, срез `v[a..b]` — zero-copy view (а не копия). Богатый
> функциональный + императивный API на уровне Kotlin/Rust, без скрытого O(n).**

---

## 1. Сравнительный анализ (что значит «не хуже»)

`core` = ядро/stdlib, `lib` = офиц. библиотека, `—` = нет.

| Возможность | Go (slices) | Rust (Vec/slice) | TS/JS (Array) | Kotlin/Java | **Nova сейчас** | **цель** |
|---|---|---|---|---|---|---|
| len / cap | `len`/`cap` | `len`/`capacity` | `length`/— | `size`/— | ✅ `len`/`cap` | ✅ |
| push/pop | `append`/slice | `push`/`pop` | `push`/`pop` | `add`/`removeAt` | ✅ | ✅ |
| insert/remove | `slices.Insert/Delete` | `insert`/`remove` | `splice` | `add(i)`/`removeAt` | ✅ | ✅ |
| swap_remove | — | `swap_remove` | — | — | ✅ | ✅ |
| swap | — | `swap` | — | `Collections.swap` | ❌ | **153.1** |
| index/get | `s[i]` | `[i]`/`get` | `[i]`/`at` | `get` | ✅ | ✅ |
| reserve/with_cap | — | `reserve`/`with_capacity` | — | `ensureCapacity` | ✅ | ✅ |
| shrink-to-fit | — | `shrink_to_fit` | — | `trimToSize` | ❌ | **= `cap(len())`** (153.1) |
| resize/truncate | — | `resize`/`truncate` | `length=` | — | ⚠ только `truncate` | **153.1** |
| clear/fill/reverse | — | `clear`/`fill`/`reverse` | `fill`/`reverse` | `clear`/`fill`/`reverse` | ✅ | ✅ |
| contains | `slices.Contains` | `contains` | `includes` | `contains` | ❌ | **153.3** |
| index_of/position | `slices.Index` | `position`/`iter().position` | `indexOf`/`findIndex` | `indexOf` | ❌ | **153.3** |
| sort / sort_by | `slices.Sort` | `sort`/`sort_by`/`sort_unstable` | `sort` | `sort`/`sorted` | ❌ | **153.3** |
| binary_search | `slices.BinarySearch` | `binary_search` | — | `binarySearch` | ❌ | **153.3** |
| dedup | `slices.Compact` | `dedup`/`dedup_by` | — | `distinct` | ❌ | **153.3** |
| partition | — | `partition` | — | `partition` | ❌ | **153.3** |
| **lazy iterator** | — | `iter()`+adapters | — (eager) | Sequence/Stream | ❌ (eager `[]T.map`) | **153.2** |
| map/filter/fold | — | adapters | core | core | ⚠ eager only | **153.2 (lazy)** |
| reduce/sum/min/max | — | `sum`/`min`/`max`/`fold` | `reduce` | `reduce`/`sumOf`/`maxOf` | ❌ | **153.2** |
| any/all/find/count | — | adapters | `some`/`every`/`find` | core | ⚠ any/all eager | **153.2** |
| take/skip/zip/enumerate | — | adapters | `slice`/— | core | ❌ | **153.2** |
| chain/flat_map/flatten | — | adapters | `flat`/`flatMap` | `flatMap`/`flatten` | ❌ | **153.2** |
| collect/for_each | — | `collect`/`for_each` | `forEach` | `toList`/`forEach` | ❌ | **153.2** |
| mut-итерация | `for i:=range`+`s[i]=` | `iter_mut` | `[i]=` | индексами | ⚠ `for mut x` | `for mut x`/`mut @iter` (153.2) |
| unzip / into_iter | — | `unzip`/`into_iter` | — | `unzip` | ❌ | **153.2-B** |
| оператор `+` (concat) | — | — | `concat` | `+` | ❌ | **`@plus` (153.5)** |
| is_sorted | `slices.IsSorted` | `is_sorted` | — | — | ❌ | **153.3** |
| **slice `v[a..b]`** | `s[a:b]` (view) | `&v[a..b]` (view) | `slice` (copy) | `subList` (view) | ✅ `[]T`-view (Plan 96) | +split_at/chunks/windows (153.4) |
| split_at | — | `split_at` | — | — | ❌ | **153.4** |
| chunks/windows | — | `chunks`/`windows` | — | `chunked`/`windowed` | ❌ | **153.4** |
| first/last/get N | `s[0]` | `first`/`last`/`get` | `at` | `first`/`last` | ✅ first/last | **153.4 (N)** |
| concat/flatten | `slices.Concat` | `concat` | `concat`/`flat` | `flatten`/`+` | ❌ | **153.5** |
| rotate | — | `rotate_left/right` | — | — | ❌ | **153.5** |
| drain | — | `drain` | `splice` | — | ❌ | **153.5** |
| extend/append | `append` | `extend`/`append` | `push(...)` | `addAll` | ⚠ дубль `append`+`extend` | **один `append`** (153.1) |
| retain/filter-inplace | `slices.DeleteFunc` | `retain` | `filter` (copy) | `retainAll` | ✅ retain | ✅ |
| **Equal/Compare** | `slices.Equal/Compare` | `PartialEq`/`Ord` | — | `equals`/`compareTo` | ✅ | ✅ |
| **Clone** | `slices.Clone` | `clone` | `[...a]` | `toList` | ✅ | ✅ |
| **Hash** | — | `Hash` | — | `hashCode` | ❌ | **153.6** |
| Display/Debug | — | `Debug` | `toString` | `toString` | ✅ | ✅ |
| FromIterator/collect-target | — | `FromIterator` | `Array.from` | `toList` | ❌ | **153.2/153.6** |

**Вывод.** Императивное ядро (push/pop/insert/index/reserve) у Nova есть. Крупные
пробелы — **(а) ленивые итераторы** (153.2, главный архитектурный лифт), **(б)
sort/search/dedup** (153.3), **(в) slice-surface** split_at/chunks/windows (153.4 —
модель `[]T`-view уже есть, Plan 96), **(г) restructure-ops**
(153.5), **(д) Hash + FromIterator** (153.6). Без них Vec «на уровне голого Go-slice»,
а не Rust/Kotlin/Java.

**Где Nova ≥ / лучше:**
- **Cost-transparency** — ленивые итераторы без промежуточных аллокаций + явная
  стоимость (лучше JS eager-`map`/`filter`, которые аллоцируют каждый шаг).
- **Типобезопасный generic-`T`** с мономорфизацией (Plan 131) — элементы в правильном
  C-типе, без int64-erasure (лучше Go `any`/interface-боксинга).
- **`[]T ≡ Vec[T]`** — один тип, не два (Rust `Vec`/`&[T]` раздвоение) при сохранении
  zero-copy views тем же типом `[]T` (cap==len), БЕЗ отдельного Slice-типа (Plan 96 D-single-type) — проще Rust.

---

## 2. Архитектура: слои

```
   Vec[T] (владеющий {data *mut T, len, cap}, на RawMem — Plan 131)
   []T  ≡  Vec[T]  (D239, чистый алиас)
        │
        ├── core/access/mutate   (153.1): push/pop/insert/remove/index/cap/swap/fill
        ├── iter (153.2): VecIter + ЛЕНИВЫЕ адаптеры (Iter/Next) → collect
        ├── sort/search (153.3): sort*/binary_search/contains/index_of/dedup
        ├── slice (153.4): v[a..b] → []T view (zero-copy, cap==len, D238/Plan 96; same type)
        ├── restructure (153.5): concat/flatten/rotate/drain/split_at
        └── protocols (153.6): Equal/Compare/Clone/Hash/Display/Debug/FromIterator
   bounds-check элизия `v[i]` — Plan 140.2 (НЕ здесь)
```

**Модульная раскладка (153.0).** `Vec` переезжает в папку `std/collections/vec/`,
модель **«папка = один модуль `collections.vec` из co-equal файлов»** (не facade-
подмодули — резолвер запрещает файл+папку одного имени, урок 152.0). Файлы по слоям
`core`/`access`/`mutate`/`iter`/`sort`/`slice`/`functional`, все `module collections.vec`.
`vec.nv` (комбинаторы) + `vec_owned.nv` (`collections.vec_owned`) **сливаются** в один
`collections.vec` (+ миграция ~6 импортов `vec_owned`→`vec`). `[]T` остаётся чистым
алиасом (D239).

---

## 2.5. Сквозные инварианты (self-consistency)

- **I1. `[]T ≡ Vec[T]`, и слайс — тот же `[]T`.** Никакого «второго типа массива» и
  **никакого отдельного `Slice[T]`** (D238 «Slice отвергнут» + Plan 96 **D-single-type**).
  `v[a..b]` = `[]T`-view (cap==len, zero-copy); мут-вариант — `mut []T` (receiver-mut),
  не `MutSlice`. Передача view в `fn f(xs []T)` работает (same type).
- **I2. Никакого скрытого O(n).** Индексация/`len`/`cap`/`push`(аморт.) — O(1);
  ленивые адаптеры — без промежуточных аллокаций; материализация только в `collect`/
  eager-терминаторах (имя явное).
- **I3. View vs owned — по конвенции `as_`/`to_`** (как в 152): `v[a..b]`/`as_slice()` —
  zero-copy view (алиас, источник переживает); `to_vec()`/`clone()` — owned.
- **I4. Один метод — один слой.** Нет дублей (eager `[]T.map` И lazy `iter().map()`
  одновременно как канон — eager либо удаляется, либо тонкий сахар над lazy, решение
  Q-iterator-laziness).
- **I5. Протоколы консистентны.** `Vec[T: Equal]`→`Equal`, `[T: Compare]`→`Compare`,
  `[T: Clone]`→`Clone`, `[T: Hash]`→`Hash`, `[T: Display/Debug]`→…; bounds на `T`.
- **I6. Без циклов модулей.** Граф `collections/vec/*` ацикличен; общий низкоуровневый
  слой — `RawMem` (не вводить internal-builder-цикл).
- **I7. Bounds-safety.** `v[i]` всегда bounds-checked (паника) до Plan 140.2; элизия
  доказуемого — 140.2, не здесь. `get`/`get(a..b)` — safe (`Option`).
- **I8. Generic-корректность.** Все методы работают для value-record/Option/tuple-`T`
  (мономорфизация Plan 131), без int64-erasure.
- **I9. Fluent-цепочки.** Мутирующие методы без data-возврата → `-> @`
  (`v.reserve(10).extend(xs).sort()`), как `StringBuilder.@append`; data-returning —
  возвращают значение. Требует устойчивого `@`-chaining (`[M-138.2-vec-self-return]`).

---

## 3. Декомпозиция (sub-plans)

Порядок: **153.0** (фундамент) → 153.1 → 153.6 → (153.2 ∥ 153.3 ∥ 153.4) → 153.5.
Фаза A — обязательный связный минимум; Фаза B — продвинутое (отделяемо).

### 153.0 — Реструктуризация модуля + `[]T≡Vec` консолидация `[engineering, A]`
Папка `std/collections/vec/`, **модель «папка = ОДИН модуль из co-equal файлов»**
(все файлы `module collections.vec`; прецедент 152.0/sync.nv — **НЕ** facade-подмодули
`collections.vec.core`, резолвер запрещает файл+папку одного имени → ambiguous). Файлы
по слоям: `core`/`access`/`mutate`/`iter`/`sort`/`slice`/`functional`, все —
`module collections.vec`.
- **Слияние двух модулей (Vec-нюанс, у str не было):** сейчас `vec_owned.nv`
  (`collections.vec_owned`, тип `Vec[T]` + методы) ⊥ `vec.nv` (`collections.vec`,
  eager-комбинаторы). Свести оба в один `collections.vec` внутри папки; удалить
  standalone `vec.nv`/`vec_owned.nv` (иначе файл+папка `vec` → ambiguous).
- **Миграция импортов (~6 сайтов):** `prelude.nv:124` (`export import
  …vec_owned.{Vec,VecIter}` → `…vec`) + 5 прямых импортёров `collections.vec_owned`
  → `collections.vec`. Тип `Vec[T]` — имя не меняется, меняется только module-path.
- **Доконсолидировать D239** (`[]T` — чистый алиас, добить Plan 138 Ф.5 если не
  завершён; убрать остаточные спец-кейсы `[]T` в компиляторе). Cross-module
  type-methods (прецедент 139.1/152.0).
- **Builder/RawMem:** если нужен общий низкоуровневый аллокатор-хелпер — приватная
  fn внутри `collections.vec` (не отдельный модуль), на RawMem.
Эстимат ~2–2.5 dd (×: миграция импортов + слияние модулей).

> #### Статус 153.0 — ✅ ЗАКРЫТ (2026-06-13, `plan-153` commit `2a5df8e4`)
>
> **Сделано.** Folder-модуль `std/collections/vec/` создан: co-equal `_module.nv`
> (`#prelude`-носитель) + `core` (тип/конструкторы/`len`/`cap`/capacity/helpers/`panic`) +
> `access` (index/get/first/last/as_ptr) + `mutate` (push/pop/insert/splice/remove/
> swap_remove/clear/truncate/reverse + bulk) + `slice` (`@index(Range)`/`@get(Range)`) +
> `iter` (VecIter+next) + `protocols` (Equal/Compare/Clone/Display/Debug). `vec_owned.nv`
> (модуль `collections.vec_owned`) ретайрнут; legacy `vec.nv` свёрнут; **~55 import-сайтов**
> мигрированы `vec_owned`→`vec`; prelude re-export'ит `Vec`/`VecIter` из folder. Модель
> folder-модуля провалидирована probe'ом (cross-file тип/методы + `#prelude` + видимость
> module-private хелперов между co-equal файлами).
>
> **Отклонение от плана (зафиксировано).** Eager-комбинаторы (`map`/`filter`/`fold`/`any`/
> `all`) НЕ свёрнуты в prelude-global folder (как буквально предлагал план «складывается в
> functional/iter»), а вынесены в ОТДЕЛЬНЫЙ explicit-import модуль `collections.vec_seq`.
> Причина: prelude-global метод вносит свои идентификаторы (метод-генерик `[Acc]`,
> callback-параметр `f`/`op`) в merged-body КАЖДОГО юнита → `[Acc]` шадовит юзерский
> `type Acc` (D145), `f`/`op` коллизит с top-level `fn f`/`fn op` (`[M-codegen-var-types-fn-scope]`;
> реальные репро в корпусе: plan138_5 `type Acc`, plan61 `fn op`). Метод-резолв в Nova
> **глобален по имени** (тело — только при импорте), поэтому import-error невозможен;
> изоляция доказана исчезновением shadow/collision. Лениво-итераторная переделка
> комбинаторов (153.2) пересмотрит, может ли lazy-слой стать prelude-global
> (`[M-153-vec-combinators-prelude-global]`).
>
> **Бонус-фиксы (user-flagged во время исполнения).** (1) `Vec[T Compare] @compare` переведён
> с байтового `RawMem.compare` (memcmp — корректно ТОЛЬКО для `Vec[u8]`; для `f64`/`int`-LE/
> record — молча неверно) на **поэлементный** lexicographic (как Rust `Vec<T:Ord>`), перенесён
> в `protocols.nv`; `@equal`/`@compare` читают оба операнда сыро без лишнего `@index`
> bounds-check. (2) Инлайн-форма `unsafe{@data[i].compare(other.data[i])}` (предложена автором)
> упиралась в **PARSER-баг D38 turbofish** (`@buf[i].compare`→`@buf::<i>.compare`) — фикс на
> ветке `plan-cgfix-erased-stub` (`6f74c0ba`, parser-correctness, 251/0), pending merge.
>
> **Критерии приёмки 153.0 (§6) — статус.**
> - ✅ `[]T ≡ Vec[T]` чистый алиас (CONFIRM; residual: явная аннотация `v Vec[int]`→`[]int`-param
>   не коэрсится, E7301, pre-existing, `[M-153-d239-explicit-vec-to-slice-param]`).
> - ✅ Модуль по слоям (folder co-equal files).
> - ✅ Ноль дублей (`vec.nv`↔`vec_owned.nv` устранён; комбинаторы — единственный экземпляр в `vec_seq`).
> - ✅ Golden существующих Vec-тестов: строгий base(main)-vs-post diff по blast-radius
>   (plan13* 191/5, plan90_1/140_2/128/99, plan91, basics/generics/plan62, plan61) — **0 регрессий**.
> - ✅ G4 (без новых FAIL по blast-radius), G6 (структурировано по слоям, координация с 140.2 соблюдена).
> - ✅ Spec D239 CONFIRM (02-types.md) + Q-vec-alias-completeness (open-questions.md) + `docs/vec-internals.md`.
> - ✅ pos+neg фикстуры `nova_tests/plan153_0/` (3/3: folder-module + alias POS, compare POS, E7301 NEG).
>
> **Открытые маркеры:** `[M-153-vec-of-variadic-codegen]`, `[M-153-d239-explicit-vec-to-slice-param]`,
> `[M-153-vec-compare-u8-memcmp-fastpath]`, `[M-153-vec-combinators-prelude-global]`,
> ~~`[M-153-scalar-min-max]`~~ ✅ CLOSED 2026-06-16. Полная история — `simplifications.md`.

### 153.1 — Core API & capacity + консолидация дублей `[D259, A]`
Добить императивное ядро до паритета: `@swap(i,j)`, `@resize(n,v)`,
`@resize_with(n, f)` (grow через closure), `@fill_with(f)` (fill через closure),
`@contains` (наив, до 153.3), capacity-инварианты. Аудит существующих (push/pop/insert/
remove/index/get/first/last/clear/truncate/reverse/fill).
**Мутация элемента — НЕ `*_mut`-аксессоры:** в value-модели Nova у `@first()`/`@get()`
нет мутабельной ссылки для возврата → мутация через `v[i] = x` (MutIndex, D240),
`mut @index`, мут-view `mut []T` (153.4). `first_mut`/`get_mut` не нужны (это Rust-borrow
артефакт). Если когда-то понадобится — только `mut @first()` (receiver-mut overload),
не отдельное имя.
**Out of scope:** front-ops (`push_front`/`pop_front`) — это `VecDeque` (отдельная
коллекция, не `Vec`); пометить `[M-153-vecdeque]`.

**Конструктор-конвенция (D259, формализована 2026-06-14):**
- **Литеральный список элементов → `Vec[T].of(a, b, c)`** (вариадик). Читается чище, чем
  `from([a, b, c])`; сужает (`Vec[u8].of(1, 2, 3)`). Аналог Rust `vec![...]`.
- **Конверсия существующей коллекции/слайса → `Vec[T].from(coll)`** (`from(items []T)`).
  Аналог Rust `Vec::from(iter)`. `from([литерал])` — **избыточен** (под D239 литерал `[a,b,c]`
  уже `Vec[T]`); док `from` (core.nv) направляет на `of`.
- **Когда тип выводится — просто `[a, b, c]`** (D239: литерал = `Vec[T]`), без `of`/`from`.
  `of`/`from` нужны лишь для inline-указания типа (return-position, generic-контекст).
- Опциональный sweep существующих `from([литерал])` → `of(...)` в тестах/stdlib —
  `[M-153.1-of-vs-from-sweep]` (низкий приоритет, churn; оба корректны).

**Accessor-конвенция (D117 AMEND — формализовать):**
- **Read-getter:** `@name() -> T => @name` (одноимённый метод без параметров читает
  поле). Снаружи — только `v.name()` (D117, `E_SIZE_ACCESSOR_FIELD` для `v.name`).
- **Write-setter:** `@name(v T)` — одноимённая перегрузка по арности — **допустим
  там, где у него есть корректная безопасная семантика под капотом** (поддерживает
  инварианты), а не «никогда для размеров».
  - **`@cap(n)` — ДА, ТОЧНО:** realloc до ёмкости **ровно `n`** (без pow2-округления —
    явный абсолютный запрос). Контракт **`n >= len`** (`n == len` валидно = zero-slack;
    `n == 0` при `len == 0` = free buffer); **`n < len` → паника**
    (ёмкость физически не меньше числа элементов; молчаливый truncate/clamp — footgun,
    ломает round-trip). **Покрывает (отдельные методы не нужны):** shrink-to-fit =
    `cap(len())`, shrink-to-min = `cap(@len().max(min))` (метод-форма `a.max(b)`, как
    `a.sin()`), room-for-N = `cap(len()+N)`.
    Держит round-trip `v.cap(n); v.cap()==n`. (pow2-округление — только неявный
    авто-рост и
    `@reserve(add)`, helper `_round_up_pow2`: bit-twiddle `v--;v|=v>>1;…;v++` или
    `clz` `1<<(64-clz(n-1))`, см. jameshfisher.com/2018/03/30/round-up-power-2; edge:
    `n<=0`, `max(8,pow2)`, overflow 2^63.)
  - **`@len(n)` — ЗАПРЕЩЁН.** Прямая установка `len` — footgun (UB при `len > cap`/
    рассинхрон с буфером; рост нечем заполнить). `@len` — **только getter**. Изменение
    размера: `@truncate(n)` (shrink), `@resize(n, v)` (grow с fill), `@push`/`@pop`.

**Fluent-конвенция (chaining, как `StringBuilder.@append`):**
- **Мутирующие `mut @...`, НЕ возвращающие данные → `-> @`** (для цепочек):
  `@cap(n)`, `@swap`, `@resize`, `@reserve`, `@sort*`, `@dedup`,
  `@rotate*`, `@fill`, `@reverse`, `@clear`, `@truncate`, `@retain` (сейчас `-> ()` —
  **поправить на `-> @`**), `@push`/`@insert`/`@append` (уже `@`). Пример:
  `v.reserve(10).extend(xs).sort()`.
- **Data-returning остаются как есть** (нельзя одновременно `@` и значение):
  `@pop->Option[T]`, `@remove->T`, `@swap_remove->T`, `@get->Option`, `@len/@cap->int`,
  `@binary_search->Result`, `@split_at->(a,b)`.
- **Зависимость:** устойчивый `@`-chaining сейчас сломан для generic-метода/value-
  record receiver — **`[M-138.2-vec-self-return]`** (цепочка мис-типизирует receiver
  в `void*`). Конвенция раскатывается **только после** фикса (153.1 Ф.0).

> **✅ Статус 153.1 codegen-полнота (2026-06-14) — критерии приёмки.** Codegen-блокеры
> chaining/overload сняты: `[M-138.2-vec-self-return]` (простой `-> @`, был закрыт 138.2+152.1)
> + `[M-138.2-generic-method-overload-mono]` (overloaded setter chain + dispatch, FIXED `f7f56f0f`)
> + `[M-153-vec-of-variadic-codegen]` (FIXED `3d9a7361`). **Критерии приёмки (все ✅, проверены
> релизным `nova`):**
> 1. Generic-type overload по **арности** в mono: `@cap()` getter vs `@cap(n)` setter
>    (`v.cap(10)` → setter, не «too many args»). Тест `plan153_1/generic_overload.nv`.
> 2. Overload по **типу аргумента** (та же арность): `@tag(int)` vs `@tag(str)` → правильное тело.
> 3. **Fluent chain** мутирующего `-> @` setter: `v.cap(n).push(x)` — return инферится как
>    mono-receiver (Self), `.push` находит метод. `generic_overload.nv` + `core_api.nv`.
> 4. **Single-overload** методы не затронуты (гейт `same_name.len()<=1`, строго no-op).
> 5. **Variadic** `Vec[T].of(...args []T)`: multi/empty/non-int собираются+диспатчат.
>    `plan153_0/variadic_of.nv`.
> 6. `@cap(n)` контракт `n>=len`: `n<len` → паника (neg-тест `cap_below_len_neg`).
> 7. **0 регрессий** (broad sweep: plan90_1/131/96/138_2/128/101/62/basics/generics/map_literals/153_0/1/6).
> Остаток (followup, не гейт приёмки): `[M-138.2-overload-no-match-typecheck]` (no-match overload →
> CC-FAIL вместо чистого type-check error); `[]int.of` array-ext-сахар (отдельный static-dispatch gap).
- **Внутри type-методов — читать ПОЛЕ напрямую (`@cap`), не getter (`@cap()`)** —
  ноль индиректности (не зависим от инлайна `=> @cap`), яснее. Getter — внешний
  контракт. (vec_owned уже так.)
- **D117 AMEND:** явно зафиксировать, что **внутренний** (type-method `SelfAccess`)
  field-read size-аккумулятора **разрешён** (`E_SIZE_ACCESSOR_FIELD` — только для
  внешних callers). Снимает противоречие комментариев в `string.nv` (Plan 152).

**Стратегия ёмкости — явное точно, неявное pow2.** Принцип: явный *абсолютный*
запрос ёмкости честится точно; неявный/амортизированный рост округляет до pow2.
- **Точные (честят `n`, без округления):** конструктор `with_capacity(n)` /
  `from_raw_parts`, **`@cap(n)`-сеттер** (shrink-to-fit = `cap(len())`, room-for-N = `cap(len()+N)`).
  Держит round-trip `v.cap(n); v.cap()==n` (accessor-конвенция) И даёт
  **предсказуемый detach слайсов** (автор точно знает точку realloc, см. 153.4).
- **Округление до pow2 (perf):** только **неявный** авто-рост на push и
  амортизированный `@reserve(add)` — через `_round_up_pow2` (степень 2 = политика
  роста для O(1)-амортизации, не явного запроса; как Rust `reserve` vs `reserve_exact`).

**Консолидация дублей API (приведение в порядок):**
- **`append` vs `extend` → один `append`.** Сейчас `@append(other Vec[T])` =
  bulk `RawMem.copy` (быстрый Vec→Vec), `@extend[S Iter[T]]` = generic per-element
  (медленный, + `@`-chaining баг). **Решение:** оставить имя **`append`**, имя
  `extend` убрать; `append` = две перегрузки — конкретная `append(Vec[T])` (bulk) +
  обобщённая `append[S Iter[T]]` (generic). Резолв: конкретный `Vec[T]` специфичнее
  generic `S` (`v.append(vec)`→bulk, `v.append(range)`→generic). **Ф.0-проверка:**
  поддерживает ли overload-резолвер Nova ранжирование «конкретное > generic»; иначе
  fallback — один generic `append` + приватный `_append_vec` fast-path. Заодно чинит
  `@`-chaining ([M-138.2-vec-self-return]). Миграция call-сайтов `extend`→`append`.
- **Аудит прочих near-дублей:** `copy_from` vs `clone`+`append` (оставить нужное);
  `splice` vs `insert`+`append`; пары `@index(panic)` ⊥ `@get(Option)` —
  **намеренные, оставить** (panic- vs safe-доступ). Документировать в `docs/vec.md`.

Эстимат ~2–2.5 dd.

> #### Статус 153.1 — 🟡 ЧАСТИЧНО (2026-06-13, main): core API + fluent ЗАКРЫТЫ; консолидация + scalar-min-max ОТЛОЖЕНЫ (codegen-лимиты)
>
> **Ф.0 (блокеры) — переоценка.** `[M-138.2-vec-self-return]` оказался **уже закрыт**
> (138.2 return-position subset + 152.1 value-record codegen, ДО написания плана 153):
> `v.push(1).push(2)`, `.reverse()`, `.fill()` цепочки работают (проба). Codegen-блокера
> НЕТ. ~~`[M-153-scalar-min-max]`~~ ✅ CLOSED 2026-06-16 (`std/runtime/defaults.nv`, все числовые типы, тест `plan153_1/scalar_min_max`); НЕ гейтит Vec-ядро.
>
> **Ф.1 (fluent) ✅.** `@reserve`, `@retain` переведены `-> ()` → `-> @` (остальные
> mut-методы уже `-> @`: push/insert/splice/clear/truncate/reverse/extend/append/copy_from/
> copy_within/fill/append_zero). Data-returning (`pop`/`remove`/`swap_remove`) оставлены.
> Цепочки `v.reserve(8).push(1).push(2)`, `v.retain(p).push(x)` — POS-фикстура зелёная.
>
> **Ф.2 (core API & capacity) ✅.** Добавлены: `@swap(i,j)`, `@resize(n,v)`, `@resize_with(n,f)`,
> `@fill_with(f)`, `@contains(v)` (наивный O(n) `==`, как `@equal`), `@cap_to(n)` точный
> capacity-сеттер (realloc до ровно `n`, контракт `n >= @len`, `n<len`→паника; покрывает
> shrink-to-fit `cap_to(len())` + room-for-N `cap_to(len()+N)`). Все mut-методы `-> @`
> (fluent). POS + 3 NEG (`cap_to<len`, `swap` OOB, `resize` neg `n` — контракт-паники)
> зелёные (`nova_tests/plan153_1/` 5/5).
>
> **Отклонение от плана (зафиксировано).** (1) **`@cap(n)`-сеттер → `@cap_to(n)`**: accessor-
> конвенция (same-name setter overload'ящий `@cap()` getter, D117 AMEND) распадается в mono —
> `v.cap(10)` мис-резолвится в 0-арг геттер ("too many arguments"), generic-method-overload-
> collapse ([M-138.2-generic-method-overload-mono], та же причина, что держит `@splice` ≠
> `@insert`). Distinct `@cap_to` маршрутизируется чисто → `[M-153.1-cap-setter-overload]`.
> (2) **Консолидация `append`/`extend` ОТЛОЖЕНА** (`[M-153.1-append-extend-consolidation]`):
> один `append` (concrete bulk + generic Iter overload) блокирован тем же overload-collapse;
> вдобавок generic-`append` (`for x in items {@push(x)}`) **ломает self-append** `v.append(v)`
> (рост во время итерации — bulk-версия снапшотит длину). Оставлены раздельно: `@append(Vec[T])`
> bulk+self-safe, `@extend[S Iter[T]]` generic.
>
> **Verify.** Blast-radius (plan90_1/plan90/plan131/plan138_2/plan128/plan153_0/plan153_1/
> contracts/basics/generics/plan62) — 0 НОВЫХ FAIL (plan131 27/1 = pre-existing vec_debug,
> чинит 154.1). std vec/*.nv читаются с диска → правки без ребилда компилятора.
>
> **Открытые маркеры:** `[M-153.1-cap-setter-overload]`, `[M-153.1-append-extend-consolidation]` (gated на codegen overload-resolver). ~~`[M-153-scalar-min-max]`~~ ✅ CLOSED. D259 spec — частичный (core API без overload-формы сеттера/консолидации).

### 153.2 — Ленивый итератор + адаптеры `[D260, Q-iterator-laziness, Q-iter-mut, A/B]`
**Главный лифт.** Ленивые адаптеры на `VecIter` (Iter/Next, D241/D242). Полный набор
паритета Rust/Kotlin-Sequence/Java-Stream:
- **Трансформ:** `map`/`filter`/`filter_map`/`flat_map`/`flatten`/`scan`/`inspect`/
  `enumerate`/`zip`/`unzip`/`chain`/`step_by`/`rev` (DoubleEnded)/`take`/`skip`/
  `take_while`/`skip_while`/`peekable`/`chunk_by`(`group_by`).
- **Терминаторы:** `fold`/`reduce`/`try_fold`/`for_each`/`collect`/`count`/`sum`/
  `product`/`min`/`max`/`min_by`/`max_by`/`min_by_key`/`max_by_key`/`find`/`find_map`/
  `position`/`any`/`all`/`nth`/`last`/`partition` (→ `(Vec,Vec)`).
- **Мутабельная итерация (Q-iter-mut):** `for mut x in v` (write-through в буфер) +
  `mut @iter()` (receiver-mut overload `@iter`, НЕ имя `iter_mut`).
- **Consuming:** `into_iter()` (consume Vec → owns элементы; для `T consume`/move).
- **collect-target:** `iter…collect() -> Vec[U]` (FromIterator, мост 153.6).

Решить eager `[]T.map` (Q-iterator-laziness). **A:** map/filter/filter_map/fold/reduce/
collect/find/any/all/count/sum/enumerate/for_each/min/max/nth/last/`mut @iter`;
**B:** zip/unzip/chain/flat_map/flatten/scan/inspect/step_by/take_while/skip_while/
peekable/min_by(_key)/max_by(_key)/partition/chunk_by/into_iter. Opus. Эстимат ~4–5 dd.

> #### Статус 153.2 — ✅ ЗАКРЫТ (Phase A) (`plan-153.2-mono-closures`, commits `996ca01a` лифт + `caf56226` ленивый слой, D260)
>
> **Сделано — продакшн-полнота без упрощений (обязательный критерий приёмки).** Ленивые
> итераторы для `Vec[T]`/`[]T` реализованы по **boxed-fluent**-модели (D260): тип-курсор
> `BoxIter[T] { priv step fn() -> Option[T] }`, вход `v.lazy()` мостит `VecIter`→`BoxIter`,
> адаптеры — fluent-методы, возвращающие новый `BoxIter` (оборачивают upstream-`step`),
> терминаторы тянут цепочку. **Промежуточных аллокаций нет** — pull-модель, по одному
> элементу на запрос (доказано инструментацией: побочный счётчик считает ТОЛЬКО реально
> протянутые элементы — `take`/`find`/`any`/`all`/`nth` коротят).
>
> - **A-набор адаптеров (✅):** `map`/`filter`/`filter_map`/`enumerate`/`take`/`skip`
>   (трансформ) + терминаторы `collect`/`fold`/`reduce`/`count`/`sum`/`any`/`all`/`find`/
>   `for_each`/`min`/`max`/`nth`/`last`. `min`/`max` — на `[T Compare]` (через
>   `x.compare(best)`, не скалярный `min`/`max`); `sum(zero T)` — без числового протокола
>   (явная аддитивная идентичность делает тип элемента и пустой результат явными).
> - **Модуль:** sibling FILE-модуль [`std/collections/vec_lazy.nv`](../../std/collections/vec_lazy.nv)
>   (`module collections.vec_lazy`), explicit-import (`import std.collections.vec_lazy`),
>   **НЕ** prelude folder `collections.vec` — закрытие closure-dense адаптерных
>   идентификаторов (`[U]`/`[Acc]`/`f`/`pred`) per [M-codegen-var-types-fn-scope]/D145, в
>   точности как `vec_seq.nv`. Eager `vec_seq` оставлен без изменений (Q-iterator-laziness:
>   lazy — канон; решение в шапке модуля).
>
> **Лифт mono×closures (codegen, `996ca01a`).** Чтобы generic-type-методы с CLOSURE-телом
> мономорфизировались корректно: gap A — `register_generic_instances_in_typeref`; gap B —
> closure-capture в loop-arms. 7/7 пробников PASS.
>
> **THREE codegen-фикса (`caf56226`, `compiler-codegen/src/codegen/emit_c.rs`, релиз
> пересобран):** (1) per-test flush реестра mut-capture box'ов `var_boxed` в `emit_test`
> (box `_box_<name>` утекал между C-функциями тестов → CC-FAIL); (2) `Stmt::Return` эмитит
> значение с типом возврата функции как target — голый `return None` в mono-замыкании
> резолвится в `NovaOpt_<mono>`, не erased `NovaOpt_nova_int`; (3) `infer_expr_c_type`
> регистрирует generic-инстанс типа-возврата, когда generic free-fn ИЛИ метод generic-типа
> возвращает generic-инстанс (`box_vec[int](it)->BoxIter[int]`, `Vec[T] @lazy()->BoxIter[T]`)
> — иначе `.method()` на временном промахивался мимо dispatch-path 5b и попадал в erased
> NULL-stub (drain 0 / segfault).
>
> **Критерии приёмки 153.2 Phase A (все ✅, проверены релизным `nova`, C-codegen):**
> 1. **Лень** — построение цепочки без терминатора НЕ выполняет работу (counter == 0);
>    `take(3)` тянет ровно 3; `find`/`any`/`all`/`nth` коротят на первом матче/промахе.
>    Тест `plan153_2/laziness` (8 кейсов).
> 2. **Композиция** — multi-stage `map→filter→collect`, `filter→map`, `skip→take`,
>    `map→filter→fold`, `filter_map`, `enumerate→map`, длинная `map→filter→skip→take`.
>    `plan153_2/chains` (10 кейсов).
> 3. **Адаптеры/терминаторы** покрыты раздельно: `plan153_2/adapters` + `plan153_2/terminators`.
> 4. **Прод-полнота без упрощений** — pull-модель real allocation-free (не материализует
>    в промежуточный Vec); адаптеры реентерабельны (каждый копирует receiver `mut src = @`
>    в свежее захватывающее замыкание — не мутирует BoxIter вызывающего до terminator-drain).
> 5. **0 регрессий** — verified pre-existing FAILs (plan103_1/expected_runtime/plan101_1/
>    range.nv) против baseline-компилятора (main `82ada7ac`).
>
> **Phase B — частично реализовано (2026-06-16):**
> - ✅ **`step_by`** — `BoxIter` + zero-cost `StepByIter` (vec_iter.nv). Тесты: `plan153_2/phase_b_lazy` + `plan153_2_zc/step_by_zc`.
> - ✅ **`chain`** — `BoxIter` (vec_lazy.nv). Тест: `plan153_2/phase_b_lazy`.
> - 🟡 **`zip`** — реализовано в vec_lazy.nv, тесты GATED `[M-153.2-tuple-elem-adapter]` (`BoxIter[(A,B)] collect` closure-typing gap).
> - 🟡 **`flat_map`** — реализовано в vec_lazy.nv, тесты GATED `[M-153.2-flat-map-inner-option]` (`Option[BoxIter[U]]` mut-match gap).
> - ❌ Остаток Phase B: `unzip`/`flatten`/`scan`/`inspect`/`take_while`/`skip_while`/`peekable`/`min_by[_key]`/`max_by[_key]`/`partition`/`chunk_by`/`into_iter`.
> + мут-итерация `for mut x`/`mut @iter()` (Q-iter-mut, отдельный write-through путь) +
> `collect`-target FromIterator (мост 153.6). Tuple-PRESERVING-адаптер сразу после
> `enumerate` (`enumerate().filter(..)`) gated на `[M-153.2-tuple-elem-adapter]`
> (residual `Option[<mono-tuple>]` closure-typing gap — схлопнуть tuple через `map` сначала).
> Zero-cost generic-over-source апгрейд поверх boxed — `[M-153.2-generic-over-source-zerocost]`.
>
> > #### Stage 4 — alloc-free терминаторы + `collect_into` (`plan-153.2-zerocost`, 2026-06-15, D277)
> >
> > **Сделано (без упрощений).** В `std/collections/vec_iter_zc.nv` добавлен терминатор
> > `mut @zcollect_into(out mut Vec[T]) -> ()` на каждый адаптер (`MapIter`/`FilterIter`/
> > `FilterMapIter`): тянет цепочку и пушит в **переданный** буфер `out` — то же тело, что
> > `zcollect`, но БЕЗ `Vec[U].new()` header-аллокации. **Семантика: APPEND** (не очищает
> > `out`; для свежего sink вызывающий делает `out.clear()` — `len=0`, буфер сохранён →
> > амортизированный ноль аллокаций при переиспользовании). Возвращает `()` (буфер виден
> > через caller-биндинг — `Vec[T]` heap-ref). **Замер из сгенерённого C** (release nova,
> > C-codegen): все четыре `…method_zcollect_into` тела = **0 `nova_alloc`** (vs `zcollect`
> > с `…_static_new()` header-аллокацией). Стриминг-терминаторы подтверждены alloc-free
> > по мономорфизированным телам: `zfold`/`zsum`/`zcount`/`zfor_each`/`zany`/`zall`/`zfind`
> > = **0 `nova_alloc`** каждый (скаляр/bool/Option-аккумулятор, без out-Vec). Остаточный
> > heap в цепочке = только env пользовательских замыканий (Stage 3,
> > `[M-153.2-closure-as-mono-type]`) + `VecIter` source-курсор (heap-ref). **Verdict:**
> > `.fold(0,…)` result-alloc = 0; `.collect_into(out)` terminator-alloc = 0. Фикстуры
> > `plan153_2_zc/collect_into` (7 кейсов) + `plan153_2_zc/streaming_terminators` (5) —
> > 2/2 PASS; plan153_2 4/4 без регрессий. Чисто stdlib-врайринг (`std/*.nv` читается с
> > диска, без ребилда компилятора).
> >
> > #### Stage 3 — devirtualizация capture-free замыканий (`plan-153.2-zerocost`, commit `44fca673`, 2026-06-15, D277)
> >
> > **Сделано (без упрощений) — alloc-elimination половина closure-devirt'а.** Замыкание
> > БЕЗ свободных переменных (env = `{int _dummy}`) **stateless** → вместо ДВУХ
> > `nova_alloc` на каждый call-site (env-box + `NovaClos_xx`-box) теперь эмитится ОДИН
> > file-scope static singleton (`nova_lambda_N_clos_singleton` + `_env_singleton`), а
> > call-site возвращает `(void*)(&singleton)`. Единственная хирургическая правка в
> > `compiler-codegen/src/codegen/emit_c.rs` (`emit_lambda`, ~line 31427): capture-free
> > fast-path. **Соундно безусловно:** stateless-замыкание байт-идентично всюду, а
> > static-адрес immortal — может escape/store/outlive любой scope (Boehm видит его как
> > root). Захватывающие замыкания (`free_vars ≠ ∅`) — heap-путь БЕЗ изменений (immutable
> > by-value snapshot + mut by-ref box требуют свежий per-instance env, singleton нельзя
> > шарить). Это **alloc-elimination** половина лифта; сам per-element ВЫЗОВ
> > `(@f)(x)` ещё идёт через `NOVA_CLOS_CALL` fn-ptr-макрос (true call-devirt =
> > `MapIter[I,T,U,F]` inline-by-value лифт → `[M-153.2-closure-as-mono-type]`).
> > **Замер из сгенерённого C** (release nova, C-codegen): канон
> > `v.ziter().zmap(|x| x*3).zfilter(|x| x%2==0).zcollect()` — closure-allocs в
> > driver-теле **4 → 0** (2 замыкания × [env-box + NovaClos-box]); та же цепочка
> > `.zfold(0, |acc,x| acc+x)` — **6 → 0** (3 замыкания × 2). Verified: 0
> > `nova_alloc(sizeof(nova_lambda_N_env))` / `nova_alloc(sizeof(NovaClos…))` в
> > сгенерённом C; все capture-free замыкания корпуса → singleton. 0 регрессий
> > (vec_iter_zc/plan153_2/plan153_2_zc/plan99/plan70/generics/contracts/plan91_fe1
> > чистые; все наблюдаемые fail'ы pre-existing на захватывающих замыканиях heap-путём
> > ИЛИ без замыканий вовсе, verified против baseline `bc4e02f5`). Маркер
> > `[M-153.2-Z-closure-devirt]` (P3, PARTIAL).
> >
> > #### Статус ступеней 3–4 + критерии приёмки 153.2-Z (zero-alloc lazy)
> >
> > | Ступень | Статус | Измеренные allocs (сгенерённый C) |
> > |---|---|---|
> > | **3 — closure devirt** | 🟡 **PARTIAL** — alloc-elimination ✅, call-инлайн ⛔ (followup) | capture-free closure env/box **4→0** (collect), **6→0** (fold); per-element вызов всё ещё fn-ptr |
> > | **4 — alloc-free терминаторы + `collect_into`** | ✅ **DONE** | `zcollect_into` тело **0 `nova_alloc`** (result-Vec устранён, reuse caller-буфера → амортизированный 0); `zfold`/`zsum`/`zcount`/`zfor_each`/`zany`/`zall`/`zfind` тела **0 `nova_alloc`** каждый |
> >
> > **Критерии приёмки 153.2-Z (все ✅, проверены релизным `nova` C-codegen):**
> > 1. **collect_into = 0 alloc** — мономорфизованные `…method_zcollect_into` тела = 0
> >    `nova_alloc` (vs `zcollect` с `…_static_new()` header-аллокацией). ✅
> > 2. **fold/стриминг = 0 alloc** — `.fold(0,…)` result-alloc = 0 (скалярный
> >    `nova_int`-возврат); все 7 стриминг-терминаторов = 0 `nova_alloc` в теле. ✅
> > 3. **closure-alloc = 0 для capture-free** — driver-тело канон-цепочки: collect 4→0,
> >    fold 6→0; ни одного closure-env/box `nova_alloc`. ✅
> > 4. **Без упрощений** — `collect_into` = реальный APPEND-терминатор в каждый адаптер
> >    (не stub); singleton-devirt соунден безусловно (не эвристика). ✅
> > 5. **0 регрессий** — plan153_2 4/4 + plan153_2_zc 2/2 PASS; cross-suite (plan139/
> >    plan147/plan124_8/plan108/plan99/plan101/plan59/plan62/basics/generics/syntax)
> >    без новых FAIL — все наблюдаемые fail'ы byte-identical против baseline `bc4e02f5`. ✅
> >
> > **Остаток (честно, НЕ упрощение):** (а) per-element ВЫЗОВ `(@f)(x)` ещё fn-ptr-индирекция
> > (true call-devirt = closures-as-mono-types, `[M-153.2-closure-as-mono-type]` P3); (б)
> > захватывающие замыкания всё ещё heap-env (per-instance, singleton нельзя шарить); (в)
> > `VecIter` source-курсор — отдельный heap-ref-type alloc на `.ziter()` (свойство
> > `VecIter[T]`, не замыкание, вне scope ступеней 3–4); (г) `take`/`skip`/`enumerate` пока
> > на boxed `vec_lazy`.
> >
> > #### Критерии приёмки Phase B adapters (2026-06-16)
> >
> > **Реализовано (проверено релизным `nova`, C-codegen, без упрощений как для прода):**
> > 1. **`step_by(n)`** — BoxIter и zero-cost StepByIter; `step_by(1)` = identity; `step_by(2)`
> >    пропускает каждый второй; `step_by(0)` → runtime panic (requires contract), проверено
> >    негативными тестами `plan153_2/step_by_zero_neg` + `plan153_2_zc/step_by_zero_neg`. ✅
> > 2. **`chain(other)`** — BoxIter; дренирует self полностью, затем other; chain с пустым
> >    first/second работает корректно; тесты `plan153_2/phase_b_lazy`. ✅
> > 3. **`zip`** — реализован в vec_lazy.nv. Тесты GATED: `BoxIter[(A,B)].collect()` требует
> >    closure-typing fix `[M-153.2-tuple-elem-adapter]`. Код в репо, задокументирован.
> > 4. **`flat_map`** — реализован в vec_lazy.nv. Тесты GATED: `Option[BoxIter[U]]` mut-match
> >    closure gap `[M-153.2-flat-map-inner-option]`. Код в репо, задокументирован.
> > 5. **`@min`/`@max` scalar** — все 12 числовых типов. `(5).max(3)==5`, chaining, neg/pos
> >    значения. Тест `plan153_1/scalar_min_max`. ✅
> > 6. **0 регрессий** — pre-existing FAILs не изменились. ✅
> > 7. **Без упрощений как для прода** (обязательный критерий):
> >    - Контракты (`requires`) на всех точках входа `step_by` — и BoxIter, и ZC.
> >    - Негативные тесты для contracts — `EXPECT_RUNTIME_PANIC requires` форма.
> >    - zip/flat_map в репо с явной документацией GATED-маркеров (не удалены, не заглушки).
> >    - scalar min/max на всех 12 типах (не только int).
> >
> > #### Статус автономного прогона 153.2 — 2026-06-14
> >
> > Лифт mono×closures (`996ca01a`) + ленивый слой (`caf56226`) закоммичены (DCO `-s`, без
> > Co-Authored-By, только intended-файлы в индексе). plan153_2 4/4 PASS (adapters/chains/
> > laziness/terminators) с корректным GC-env (worktree-setup). 0 регрессий по blast-radius.
> > Сопутствующие bug-фиксы той же ветки: `ffc5d28f` (std.sort/binary_search консолидация,
> > Bug A) + `cf3951e2` (chained `Vec[f32]` turbofish-static return-inference, Bug B).

### 153.3 — Sort & search `[D261, A]`
`@sort()`/`@sort_by(cmp)`/`@sort_by_key(key)` (stable; + `@sort_unstable*`),
`@is_sorted`/`@is_sorted_by`, `@binary_search(x)`/`@binary_search_by`/
`@binary_search_by_key`, `@contains`, `@index_of`/`@position(pred)`/`@rposition`,
`@dedup`/`@dedup_by`/`@dedup_by_key`, `@partition(pred)` (in-place split). Bounds на
`T: Compare` где нужно. **Roadmap:** `@select_nth_unstable` (quickselect, `[M-153-select-nth]`).
Эстимат ~2–3 dd.

> **✅ ВЫПОЛНЕНО 2026-06-14** (commits `cf95c423` search + `1d85edc3` sort/dedup/partition).
> 18 методов: search (`@index_of`/`@position`/`@rposition`/`@is_sorted[_by]`/
> `@binary_search[_by][_by_key]`), sort (`@sort`/`@sort_by`/`@sort_by_key` + 3 `*_unstable`),
> reorder (`@dedup`/`@dedup_by`/`@dedup_by_key`/`@partition`). `@contains` уже был (153.1).
> **Критерии приёмки (все ✅, проверены релизным nova):** (1) `@sort` — bottom-up **STABLE**
> merge sort (O(n log n)): ascending + дубли(стабильность) + пусто/один; (2) `@sort_by`/
> `@sort_by_key` — кастомный comparator/key через Nova-замыкание; (3) `@binary_search`
> Ok=found/Err=insert-point, `@is_sorted` по adjacent-парам; (4) `@dedup*` consecutive +
> `v.sort().dedup()`=unique (fluent chain); (5) `@partition(pred)->int` split-point, satisfying-
> первыми; (6) **0 регрессий** (vec-sanity: plan153_0/1/6 + plan90/90_1 чисты). Тесты plan153_3:
> search 4/4 + sort 5/5 + dedup_partition 5/5 + heapsort_rigor 5/5 + select_nth 4/4 + OOB-neg.
> **Production-grade без упрощений (commit `468bccf5`):** `@sort_unstable*` — настоящий **in-place
> heapsort** (O(n log n) worst, O(1) extra), НЕ alias стабильного; `@select_nth_unstable` —
> **introselect** (median-of-three quickselect + heapsort depth-guard, O(n) avg / O(n log n) worst,
> контракт `k∈[0,len)`). Оба — реальные алгоритмы. **`binary_search() == Ok(x)` / `== Err(x)` для
> non-default-E ✅ РАБОТАЕТ** (`[M-153-result-eq-literal-expected-type]` RESOLVED — codegen
> переэмитит голый `Ok/Err`-литерал под concrete Result-тип операнда; тест `result_eq_literal`).
> **Остаток (perf-only, не упрощение):** pdqsort поверх heapsort (`[M-153.3-sort-pdqsort]`).

### 153.4 — Слайсы и views (достроить на модели Plan 96) `[D262, A/B]`
**Модель уже принята и приземлена** (D238 + Plan 96 **D-single-type** + **D-cap-len**):
`v[a..b]` = zero-copy view **того же типа** `[]T`=`Vec[T]` (`{data:@data+a, len, cap:len}`,
cap==len), НЕ отдельный `Slice`-тип. 153.4 = **подтвердить + достроить недостающее:**
`@split_at(i) -> ([]T, []T)`, `@split_first()`/`@split_last() -> Option[(T, []T)]`,
`@chunks(n)`/`@chunks_exact(n)`/`@rchunks(n) -> [][]T`, `@windows(n)`, `@first_n`/
`@last_n`, `@as_slice() -> []T` (+ `mut @as_slice()` — мут-view через **receiver-mut
overload**, как `@as_ptr`/`mut @as_ptr`, Plan 135/D247, НЕ имя `as_mut_slice`). Все
возвращают `[]T`-views. **Решить:** `chunks`/`windows` — ленивый итератор (153.2-стиль)
vs eager `[][]T`; рекомендация — **ленивый** (без аллокации внешнего Vec), как Rust.
Opus. Эстимат ~2–2.5 dd (модель готова).

**Detach-on-resize (Go-модель, GC-safe) — уже D238/Plan 96.** View с `cap==len`
алиасит буфер мастера; **push на view** → `cap==len` → realloc → **silent detach**
(view получает свой буфер, родительский backing не перезаписывается — Go-footgun
устранён без borrow-checker). Предсказуемость точки detach у мастера — через **точную
ёмкость** (`with_capacity`/`@cap(n)`, 153.1). Мут-view (`mut []T`/`for mut x`) —
write-through до detach. Подтвердить в доках; новых решений не требуется.

> #### Статус 153.4 — ✅ ЗАКРЫТ ЦЕЛИКОМ (A: eager-views 2026-06-14 `plan-153.4-slices`; B: lazy chunks/windows 2026-06-15 `plan-153-wave`)
>
> **153.4-A ✅ (eager zero-copy `[]T`-views, БЕЗ внешней аллокации).** Новый peer-файл
> `std/collections/vec/views.nv` (folder-module `collections.vec`): `@split_at(i) -> (Self,Self)`
> (два view, `requires 0<=i<=len`, OOB→panic, НЕ clamp); `@split_first()`/`@split_last() ->
> Option[(T, Self)]` (голова/хвост + view, пусто→`None`); `@first_n(n)`/`@last_n(n) -> Self`
> (префикс/суффикс, **CLAMP** `n>len`→весь, `n<=0`→пусто — «take up to N» никогда не сюрпризит);
> `@as_slice() -> Self` (ro-view всего) + `mut @as_slice() -> mut Self` (мут-view через
> **receiver-mut overload**, как `@as_ptr`/`mut @as_ptr` — НЕ имя `as_mut_slice`, D247/Plan 135).
> Все возвращают `[]T`≡`Vec[T]`-views (zero-copy, `cap==len`); detach-on-resize подтверждён тестом
> (`first_n→push` детачит, родитель не тронут). Тесты `plan153_4/views` (14 `test`-блоков:
> split_at делит + invariant `len(l)+len(r)==len` + boundaries 0/len + write-through;
> split_first/last non-empty + empty→None + single; first_n/last_n exact+clamp+empty;
> mut as_slice write-through; detach) + негатив `plan153_4/split_at_oob_neg` (EXPECT_RUNTIME_PANIC).
> Все PASS через релизный nova (C-codegen).
>
> **Codegen-фикс (без упрощений).** Return-type inference генерик-инстанс-методов (emit_c.rs
> `infer_expr_c_type`, generic-type-instance fallback ~33073) НЕ резолвил `Self`, ВЛОЖЕННЫЙ в
> композит (`(Self,Self)`, `Option[(T,Self)]`): `subst` нёс только generic-параметры (`T`), а
> top-level-`Self` обрабатывался отдельным `.or_else`, который не рекурсировал в tuple/Option
> элементы → локал `let (l,r)=v.split_at(i)` объявлялся с ГЕНЕРИК-tuple `Nova_Vec*` (vs mono
> `Nova_Vec_int*` из callee) → C «incompatible tuple type». Фикс: `subst.push(("Self", mono-receiver))`
> перед `apply_type_subst_to_ref` → вложенный `Self` резолвится. Аддитивно (резолвит там, где раньше
> `None`); 0 регрессий (plan153_0/1/3/6, generics, basics, plan131/138/90_1 чисто; plan138_2 4-FAIL
> идентичны baseline = pre-existing Vec-shadow E7310 + transient lld-link file-lock).
>
> **153.4-B ✅ ЗАКРЫТА (2026-06-15, ветка plan-153-wave) — `[M-153.4-chunks-windows-lazy]` CLOSED.**
> `@chunks(n)` / `@chunks_exact(n)` / `@rchunks(n)` / `@windows(n)` реализованы **ленивыми** итераторами
> (рекомендация плана; Rust/Kotlin-стиль, БЕЗ аллокации внешнего `[][]T`-Vec) поверх ленивой инфры
> Plan 153.2. Каждый — инстанс-метод `Vec[T] @… -> BoxIter[Self]` в `std/collections/vec_lazy.nv`
> (sibling-файл, НЕ prelude `vec/`-папка: bodies форвардят capturing-closure → generics-leak D145, как
> все адаптеры → opt-in `import std.collections.vec_lazy`), yield'ящий zero-copy `[]T`≡`Vec[T]`-views
> (`src[a..b]`, `cap==len`) на том же буфере (Plan 96 D-single-type/D238). `collect()` материализует
> `[][]T` (`Vec[Vec[T]]`) только по требованию; `chunks(n).map(|w| w.len())` / `.fold` / `.count` /
> `.for_each` — без аллокации внешнего Vec вовсе. Семантика: `chunks` непересек., last короткий;
> `chunks_exact` дропает короткий хвост; `rchunks` с конца (leading короткий, yield back-to-front);
> `windows` перекрывающиеся width-n (n−1 общих с соседним), `n>len`→пусто. Контракт `n > 0` (`requires`,
> runtime-panic). **Без compiler-фикса** — early-exhaustion возвращает `ro done Option[Self] = None`
> (типизированный локал), т.к. bare `return None` в closure с КОНКРЕТНЫМ `Vec[T]` элементом (не свободный
> generic) монофится в дефолтный `Option[<elem>]` и расходится со step-return `BoxIter[Self]` (тот же
> класс что `[M-153.2-tuple-elem-adapter]`). Тесты `plan153_4/chunks_windows` (23 test-блока) + 4 негатива
> (`chunks_zero_neg`/`chunks_exact_zero_neg`/`rchunks_zero_neg`/`windows_neg`, EXPECT_RUNTIME_PANIC
> requires) + smoke в самом vec_lazy.nv. Верификация (релизный nova C-codegen, env GC на main repo):
> **plan153_4 7/0, plan153_2 4/0, plan96 23/0, plan153_0 4/0, plan153_1 7/0, basics 8/0, plan131 28/0,
> plan138 10/0 = 0 регрессий.**
>
> **Критерии приёмки 153.4-A (все ✅).**
> 1. Slice-поверхность построена на `[]T`-view модели (D238/D239 + Plan 96), **БЕЗ** нового
>    `Slice`-типа — view = `Vec`-заголовок `cap==len`, same-type, zero-copy. ✅
> 2. `@split_at` (контракт `0<=i<=len`, OOB→panic, инвариант `len(l)+len(r)==len`),
>    `@split_first`/`@split_last` (пусто→None), `@first_n`/`@last_n` (clamp «до N»),
>    `@as_slice` (ro) + `mut @as_slice` (write-through, recv-mut overload). ✅
> 3. detach-on-resize подтверждён тестом (`first_n→push` детачит, родитель не тронут). ✅
> 4. chunks/windows честно отложены ленивыми (`[M-153.4-chunks-windows-lazy]`, gated 153.2),
>    НЕ реализованы наспех eager. ✅
> 5. D262 зафиксирован (spec/decisions/03-syntax.md); доки обновлены (vec-internals.md
>    «Slices & views» + module-layout `views.nv`; collections/vec-owned.md slice-таблица+пример). ✅
> 6. Все запрошенные suite зелёные через релизный nova (C-codegen). ✅
>
> **Вердикт 153.4: 🟢 A ЗАКРЫТА, B отложена.** Eager zero-copy `[]T`-view поверхность готова
> и зашиплена; B (ленивые chunks/windows) — gated на Plan 153.2 за `[M-153.4-chunks-windows-lazy]`.
> Верификация (2026-06-14, релизный nova C-codegen, env `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`
> на main repo): **plan153_4 2/0, plan96 23/0, plan153_0 4/0, plan153_1 7/0, plan138 10/0,
> basics 8/0 = 54 PASS / 0 FAIL**. 0 регрессий.

### 153.5 — Restructure-ops + оператор `+` `[D263, Q-vec-operator-plus, B]`
`@concat(other) -> Vec[T]` + **оператор `+`** (`@plus`, `a + b` = новый Vec, как
str `@plus`; Q-vec-operator-plus), `[][]T.flatten()`, `@rotate_left(n)`/
`@rotate_right(n)`, `@drain(range) -> Vec[T]` (вырезать диапазон, вернуть владеемый),
`@insert_slice(i, sl)`. (extend/append/retain/splice — уже есть, аудит.) Эстимат ~1.5–2 dd.

> #### Статус 153.5 — ✅ ЗАКРЫТ (2026-06-14, `plan-153.5-restructure` commit `e8f700e4`)
>
> **Сделано (production-grade, без упрощений).** Новый слой
> [`std/collections/vec/restructure.nv`](../../std/collections/vec/restructure.nv) (co-equal
> файл folder-модуля `collections.vec`), все методы на Nova-body поверх bulk `RawMem.copy`:
> - **`@concat(other) -> Vec[T]`** — non-mutating join (НЕ `@append`, который растит `self`
>   in place): одна аллокация ровно на `a + b` элементов (`with_capacity(a+b)`), затем два
>   bulk-`RawMem.copy` прохода; операнды нетронуты. Семантика Kotlin/Python `+`, Rust `concat`.
> - **Оператор `+`** — `export fn Vec[T] @plus(other) -> Vec[T] => @concat(other)`; `a + b` =
>   НОВЫЙ Vec (как str `@plus`, D46). `a += b` лоуэрится в `a = a + b` (свежий concat-Vec) —
>   рост `a` in place остаётся за `a.append(b)`. Codegen-поддержка `+`/`+=` добавлена в
>   `emit_c.rs` (см. ниже).
> - **`mut @rotate_left(n)` / `mut @rotate_right(n) -> @`** — циклический сдвиг in place.
>   `n` редуцируется `mod len` (любой `n >= 0` валиден; полный/много-оборотный = identity).
>   O(len) time, O(min(n, len−n)) scratch; overlap-safe; right ≡ left на `len − k`. Контракт
>   `requires n >= 0`. Пустой/одноэлементный — без изменений.
> - **`mut @drain(range Range) -> Vec[T]`** — вырезать `[start, end)`, вернуть удалённое
>   как НОВЫЙ владеемый `Vec[T]`; суффикс сдвигается вниз (overlap-safe), `self` короче на
>   `range.len()`. O(len). Контракт `requires start>=0 && end>=start && end<=@len`. Пустой
>   диапазон → пустой Vec, `self` нетронут.
> - **`mut @insert_slice(i, sl []T) -> @`** — вставить срез `sl` на индекс `i` (`i==len` =
>   append); под D239 `[]T` ЕСТЬ `Vec[T]`, поэтому делегирует в `@splice` (overlap-safe →
>   self-insert корректен). Имя документирует slice-аргумент (Rust `splice` / Go `slices.Insert`).
>   Контракт `requires 0 <= i && i <= @len`.
>
> **Codegen (`emit_c.rs`, +68).** Две точки для оператор-перегруженного `+`/`+=`, обе
> минимально-таргетные (не трогают struct-tag / protocol-resolve / prelude / Option-arg —
> ортогональны pre-existing plan62-фейлам):
> 1. `Stmt::Assign` arm: `a += b` / `a -= b` на типе с method-лоуэрингом `+` (`nova_str`,
>    `Nova_Vec____*`, любой `Nova_*`-record с `@plus`) десугарится в синтез-`Binary{Add}` →
>    re-emit через полный binop-dispatch (а не сырой C `a += b`, нелегальный на struct/pointer
>    операнде → CC-FAIL). Только `Add`/`Sub` перегружаемы.
> 2. `BinOp::Add` arm: `Vec[T] + Vec[T]` маршрутизируется через `vec_method_call(.., "plus", ..)`
>    ПЕРЕД generic-`Nova_*`-sum-pointer Add-arm (тот эмитит голый `_method_plus(l, r)` без
>    инстанцирования mono-тела → undefined symbol при линковке). `vec_method_call` регистрирует
>    mono-инстанс первым.
>
> **flatten — ✅ РЕАЛИЗОВАН (followup 2026-06-14, commits `1c323d0e` + `16753d23`).**
> `[][]T.flatten() -> []T` (production carrier-форма `Vec[Vec[T]] @flatten() -> Vec[T]`)
> залендён в [restructure.nv](../../std/collections/vec/restructure.nv) вместе с фундаментом
> **вложенных generic-ресиверов произвольной глубины** — `[M-153.5-flatten-nested-receiver]`
> **ЗАКРЫТ**. flatten pre-size'ит `out = with_capacity(Σ inner.len())`, затем bulk-копирует
> каждый ряд `out.append(inner)` (тот же `RawMem.copy` fast-path).
>
> **Root-cause фикса (обе половины — НЕ упрощение, глубокий compiler-fix).** Раньше обе формы
> ресивера теряли вложенность: (1) ПАРСЕР отвергал carrier `Vec[Vec[T]]` («expected `]`») и
> схлопывал `[][]T`→`"[]T"`; (2) МОНОРФИЗАТОР биндил receiver-typevar `T` в *непосредственный*
> элемент (`Vec[int]`), не во *внутренний* (`int`) → неверный return-тип + segfault на
> индексации (verified probe был RUN-FAIL, mono'd `out` = `Nova_Vec____Nova_Vec____nova_int_p`).
> **Фикс (рекурсивный, depth-agnostic, без one-level-hardcoding):** AST-носитель
> `Receiver.receiver_ty: Option<TypeRef>` (полный структурированный тип — `type_name`
> flatten'ит и теряет глубину); парсер принимает вложенный `parse_type` в carrier-слоте +
> рекурсивный сбор free-typevars (carrier) и считает глубину `[]` + спуск до внутреннего
> `Named` (slice); монорфизатор переиспользует рекурсивный `infer_type_param_binding` (bind
> `T` = innermost element на любой глубине) на ВСЕХ путях receiver-typevar-bind + depth-aware
> sentinel-ключи `"[]"*N+"T"`. Flat `[]T` (depth 1) остался **byte-identical** (legacy
> `NovaArray_`-путь); override гейтнут `receiver_ty_is_nested`. Checker collect'ит вложенные
> typevar'ы для `E_UNUSED_PREFIX_TYPEVAR`, но НЕ сидит scope из `receiver_ty` (сохраняет
> `E_UNDECLARED_TYPEVAR_IN_RECEIVER` — seed был бы регрессией, verified). Spec — D145 AMEND
> (02-types) + D263 AMEND (10-overloading).
>
> **Compiler-bug по дороге (FIXED, не упрощение).** `@data + @len` внутри Vec-методов на
> `Vec[Vec[T]]` (где `data` = `*mut Vec[T]`, лоуэрится в одиночный `Nova_Vec____*`) мис-
> диспатчил `ptr + int` в pointee-`@plus`(=`@concat`) → segfault; emit_c.rs Add-арм (Vec-plus +
> generic sum-plus) теперь требует ОБА операнда matching record/Vec value-типа, `ptr + int`
> падает в типизированную pointer-арифметику (verified: scalar-операнд `@plus(int)` overload'ов
> нигде нет).
>
> **Cross-cutting + честно про остаток.** Это путь, через который идут ВСЕ `[]T`-методы stdlib —
> изменение специально гейтнуто на genuinely-nested ресиверы, flat-случай неизменен (0 регрессий
> по slice/vec/generic-surface). **Ортогональный pre-existing остаток (вне scope):** slice-форма
> `fn[T] [][]T -> []T`, чьё тело СТРОИТ свежий `Vec[T].new()`, упирается в pre-existing erased-
> base-body лимит, который ЛОМАЕТ и flat `fn[T] []T` с `Vec[T].new()` на baseline. Production-
> flatten — CARRIER-форма (как все stdlib), работает полностью; slice-form nested-receiver
> binding доказан отдельно (`@count_all`/`@first_row`).
>
> **Тесты flatten/nested:** plan153_5_nested 4/4 (`flatten_depth2` Vec[Vec[T]]→Vec[T] int+str,
> `flatten_depth3` depth≥3 + nested-typed return, `slice_nested` `@count_all`/`@first_row`,
> `control_flat` flat unchanged) + plan153_5/`flatten` (positive `[[1,2],[3],[4,5]].flatten()==
> [1,2,3,4,5]`, empty rows, empty outer, str-elements, double-flatten `Vec[Vec[Vec[int]]]`) +
> `flatten_plus_guard` (operator-`+` регрессия-гард).
>
> **Критерии приёмки 153.5 (все ✅, проверены релизным `nova` C-codegen).**
> 1. **`@concat` / `+` не мутируют операнды** — `c = a.concat(b)` и `d = a + b` дают
>    `[1,2,3,4,5]`, при этом `a.len()==3`, `b.len()==2` (тест `restructure.nv` POS).
> 2. **`+=` = append-семантика через concat** — `a += b` даёт конкатенацию (свежий буфер; тест
>    `restructure.nv` «operator += appends»).
> 3. **`rotate_left`/`rotate_right` обратимы + `n%len==0` identity** — `[1..5].rotate_left(2)` =
>    `[3,4,5,1,2]`, `.rotate_right(2)` восстанавливает; `rotate_left(3)`/`(6)` на `[1,2,3]` = no-op.
> 4. **`drain` возвращает вырез + укорачивает + пустой диапазон** — `v.drain(1..4)` = `[2,3,4]`,
>    `v` = `[1,5]`; `u.drain(1..1)` = пусто, `u` нетронут.
> 5. **`insert_slice` середина + конец(=append)** — `[1,2,5,6].insert_slice(2,[3,4])` =
>    `[1,2,3,4,5,6]`; вставка на `len` = append.
> 6. **Контракт-паники (NEG, runtime-panic фикстуры):** `drain` OOB (`drain_oob_neg`), `drain`
>    reversed-range (`drain_reversed_neg`), `insert_slice` `i>len` (`insert_slice_oob_neg`),
>    `rotate` отрицательный `n` (`rotate_negative_neg`) — все паникуют по `requires`.
> 7. **0 регрессий от 153.5.** Релизным `nova` прогнаны 7 сьютов: plan153_5 5/5, plan90 9/9,
>    plan90_1 21/21, plan153_0 4/4, plan153_1 7/7, basics 8/8, plan62 29/7. Все 7 plan62-FAIL —
>    **PRE-EXISTING** (НЕ регрессии 153.5): доказано baseline-бинарём на родительском коммите
>    `c0f269dd` (один до `e8f700e4`) в temp-worktree — ИДЕНТИЧНЫЕ 29/7, те же имена тестов и те же
>    категории ошибок (prelude/module/protocol struct-tag — ортогональны restructure-ops; единственное
>    не-аддитивное изменение `e8f700e4` = `emit_c.rs` +/+= handling, не трогает их кодпути). Temp-
>    worktree удалён + pruned.
>
> **Бинарь актуален.** Релизный `nova` новее всех `.rs`-исходников; `std/collections/vec/
> restructure.nv` грузится с диска при компиляции (не `include_str!`), совпадает с HEAD.
>
> **Spec / Q.** D263 (restructure-ops + оператор `+`) записан в [10-overloading.md](../../spec/decisions/10-overloading.md)
> + **D263 AMEND** (flatten реализован) + **D145 AMEND** (02-types: вложенные generic-ресиверы
> произвольной глубины, фундамент flatten); Q-vec-operator-plus → ✅ ЗАКРЫТО в
> [open-questions.md](../../spec/open-questions.md). Гайд — `docs/strings.md`-аналог: раздел
> concat/+/restructure/flatten + заметка о вложенных ресиверах в [vec-internals.md](../vec-internals.md).
>
> **Открытые маркеры:** нет (все 153.5-маркеры закрыты;
> `[M-153.5-flatten-nested-receiver]` ✅ РАЗРЕШЁН followup'ом 2026-06-14 — вложенные
> generic-ресиверы + flatten). Полная история — `simplifications.md`.

### 153.6 — Протоколы (Equal/Compare/Clone/**Hash**/Display/Debug/FromIterator) `[D264, A]`
Добавить `Vec[T: Hash] @hash()` (для `HashSet[Vec]`/ключа); закрепить `FromIterator`/
`collect`-target (мост с 153.2); аудит consistency Equal/Compare/Clone/Display/Debug
(уже есть). Эстимат ~1.5 dd.

> #### Статус 153.6 — ✅ ЗАКРЫТ (Hash 2026-06-13 + FromIterator/collect-target 2026-06-14, D264); HashMap-Vec-key отложен
>
> **Hash ✅.** `Vec[T Hash] @hash() -> u64` (protocols.nv) — FNV-1a (64-bit): fold длины +
> per-element `@hash()`, `h = (h ^ x) * prime`. u64-mul **врапается** (Nova-семантика, проверено)
> = FNV mixing-шаг; offset basis — **hex**-литерал (десятичная форма > i64::MAX, парсилась бы
> как `int` с overflow). Consistency с `@equal` (равные Vec → равный hash). plan153_6/hash 3/3
> (equal-hash, empty/single round-trip, content+length distinguish).
>
> **FromIterator / collect-target ✅ (2026-06-14, D264, `[M-153.6-fromiterator-gated]` ЗАКРЫТ).**
> Nova типизирует итераторы **структурно** (D58: любой `mut @next()->Option[T]` итерируем),
> поэтому FromIterator — НЕ enforced-протокол с одним методом, а **набор конструкторов/терминаторов**
> (паритет Rust `collect`/`FromIterator`/`extend` поверх существующей инфры):
> - **Default collect-target → `Vec`:** `BoxIter[T] mut @collect() -> Vec[T]` (ленивый слой
>   153.2, уже был) — материализует pipeline в один проход без промежуточного `Vec` на стадию.
> - **`Set` collect-target (NEW):** `BoxIter[T Hash] mut @collect_set() -> Set[T]` (vec_lazy.nv,
>   dedup; Rust `iter.collect::<HashSet<_>>()`). Allocation-free над pipeline (pull+insert).
> - **Прочие таргеты — композицией над собранным `Vec`:** `Set[T].from_iter(it.collect())`,
>   `HashMap[K,V].from(pairs.collect())` (под D239 собранный `Vec` ЕСТЬ `[]T`-аргумент `from_iter`).
> - **FromIterator из произвольного `Iter`-источника (без ленивой стадии):**
>   `Vec[T].new().extend(src)` — instance `@extend[S Iter[T]]` мономорфизируется корректно для
>   любого `S` (Range/VecIter/Vec); прямой call-site идиом без обёртки.
>
> **Критерии приёмки 153.6 FromIterator (все ✅, проверены релизным nova, C-codegen):**
> 1. **Default target — Vec:** `src.lazy().map().filter().collect()` = ожидаемый Vec; пустой
>    pipeline → пустой Vec. 2. **`@collect_set` дедуп:** int + str элементы, после `map`/`filter`,
>    пустой → пустой Set. 3. **collect-в-Set/HashMap композицией:** `Set.from_iter(it.collect())`
>    + `HashMap.from(pairs.collect())`. 4. **FromIterator из источника:** `extend(0..5)` (Range),
>    `extend(vec)` (Vec), `extend(collected)` (VecIter). 5. **0 регрессий** (plan153_2 4/4,
>    plan153_0/1/3/4/5, plan96 23/23, set/hashmap/vec_seq stdlib, basics 8/8; plan62 29/7
>    PRE-EXISTING struct-tag/protocol — не импортит vec_lazy, доказано). Тесты plan153_6/collect_target
>    12/12 + collect_static_generic_neg (NEG).
>
> **Gated (compiler-gaps, НЕ упрощение).** (1) **`[M-153.6-collect-static-generic]`** — *статический*
> generic-конструктор `Vec[T].from_iter[S Iter[T]](src S)` с for-in по `S` НЕ компилируется (bound
> `S Iter[T]` не резолвится для for-in dispatch в static generic-методе; CODEGEN-FAIL). Instance-`@extend`
> — рабочий обход; NEG-фикстура `collect_static_generic_neg` лочит границу. (2) **`[M-153.6-collect-map-tuple-receiver]`**
> — прямой `BoxIter[(K,V)] @collect_map()->HashMap` НЕ парсится (receiver type-арг кортежем).
> HashMap collect-target = `HashMap.from(pipeline.collect())`.
>
> **Отложено (вне scope 153.6).** **Vec как ключ HashMap/HashSet** — `[M-153.6-vec-hashmap-key-eq]`:
> pre-existing HashMap-codegen-баг — collision-check `k.eq(key)` (hashmap.nv) НЕ диспатчит в
> Vec-`@equal` (D237 eq→equal; generic-type gap) → «no member named eq». `@hash` готов, это
> equality-половина ключ-контракта.
>
> **Аудит consistency.** Equal/Compare/Clone/Display/Debug на месте (153.0); Hash добавлен
> consistent с Equal; FromIterator/collect-target поверх ленивого слоя (D260). **D264 — записан
> полностью** (spec/decisions/02-types.md: Hash + FromIterator/collect-target).

---

## 3А. Фазы по приоритету — «сейчас» vs «позже»

**Phase A** (обязательно, связный минимум — Vec не хуже Go/Rust-core по императиву +
базовым итераторам/сортировке): 153.0, 153.1, 153.2-A, 153.3, 153.6, 153.4-A (`as_slice`/
`split_at`/`v[a..b]`-view). **Phase B** (продвинутое, отделяемо): 153.2-B (zip/chain/
flat_map/…), 153.4-B (chunks/windows/mut-view), 153.5 (concat/rotate/drain).

**Acceptance Phase A:** `[]T≡Vec` консолидирован; императивное ядро + sort/search +
базовые ленивые адаптеры + Hash + zero-copy `v[a..b]`; полный `nova test` зелёный.
Без B Vec честно «core-complete», B-пробелы помечены `[M-153-*]`.

---

## 4. Spec / D / Q / документация (обязательные deliverables)

**Решения (D) — резерв D259–D266:**
- **D259** (NEW) — Vec core API & capacity (swap/resize/cap-exact, reserve).
- **D260** (NEW) — ленивый итератор + адаптеры (model + Iter/Next интеграция).
- **D261** (NEW) — sort & search (stable/unstable, binary_search, dedup).
- **D262** (✅ IMPLEMENTED ЦЕЛИКОМ — A 2026-06-14, B 2026-06-15, минорный) — slice-op surface (split_at/first_n/last_n/as_slice eager + chunks/chunks_exact/rchunks/windows **ленивые** `-> BoxIter[Self]`) на `[]T`-view модели D238/Plan 96 (БЕЗ новых типов; подтверждает single-type). Зафиксирован в spec/decisions/03-syntax.md#d262; `[M-153.4-chunks-windows-lazy]` ✅ CLOSED.
- **D263** (✅ IMPLEMENTED 2026-06-14) — restructure-ops (concat/`+`/rotate/drain/insert_slice + **flatten**) — записан ([10-overloading.md](../../spec/decisions/10-overloading.md)); flatten реализован через вложенные generic-ресиверы произвольной глубины (D145/D263 AMEND, `[M-153.5-flatten-nested-receiver]` ✅ ЗАКРЫТ).
- **D264** (✅ IMPLEMENTED 2026-06-14) — Vec-протоколы (`Hash` + FromIterator/collect-target) —
  записан в [02-types.md](../../spec/decisions/02-types.md#d264-vec-протоколы-hash--fromiterator--collect-target-plan-1536).
- **D239 AMEND/CONFIRM** — `[]T` чистый алиас завершён (Plan 138 Ф.5 закрыт).
- **D117 AMEND** — accessor-конвенция: read-getter `@name()=>@name`, write-setter
  `@name(v)` где есть безопасная семантика под капотом (`@cap(n)` → realloc ТОЧНО до
  n, контракт `n>=len`, round-trip; `@len(n)` — ЗАПРЕЩЁН, footgun →
  `truncate`/`resize`/`push`/`pop`); внутри
  type-метода field-read size-аккумулятора разрешён (E_SIZE_ACCESSOR_FIELD — только
  внешним callers). Кросс-план: применить и в Plan 152 (str).
- **D238/D240 AMEND** (при необходимости) — `Index[Range]` со `str`-подобной view-
  семантикой для Vec.

**Открытые вопросы (Q) — закрываются В ПЛАНЕ записями-решениями:**
- **Q-iterator-laziness** (NEW) — **ЗАКРЫТО: ленивые адаптеры — канон** (Rust/Kotlin-
  Sequence/Java-Stream); материализация только `collect`/eager-терминаторами. Старый
  eager `[]T.map(f)->[]U` → тонкий сахар над `iter().map().collect()` ИЛИ deprecate
  (решить в 153.2 Ф.0; рекомендация — сахар, чтобы не ломать call-сайты).
- **Q-slice-view** — **НЕ открытый вопрос: УЖЕ РЕШЕНО** (D238 + Plan 96 **D-single-
  type** + **D-cap-len**). `v[a..b]` = zero-copy view **того же типа** `[]T`=`Vec[T]`
  (cap==len), **НЕ отдельный `Slice`-тип** (D238 «Slice отвергнут»). owned — явный
  `to_vec()`/`clone()`. **Detach-on-resize (Go-модель):** push на view (cap==len) →
  realloc → отвязка в свой буфер, родитель не перезаписан (Go-footgun устранён без
  borrow-checker). Предсказуемость точки detach у мастера — через **точную ёмкость**
  (`with_capacity`/`@cap(n)`, 153.1) → поэтому явная ёмкость **не округляется** до
  pow2. 153.4 лишь **подтверждает + достраивает** split_at/chunks/windows. Согласуется
  со str-линзой `as_bytes() -> ro []u8` (152) — тоже same-type view, не отдельный тип.
- **Q-vec-alias-completeness** (NEW) — **ЗАКРЫТО: `[]T` — чистый алиас** `Vec[T]`,
  раскрывается на type-resolution (D239); остаточные спец-кейсы убрать в 153.0.
- **Q-vec-mutability-through-view** — **РЕШЕНО (Plan 96):** мут-view — `mut []T`
  (receiver-mut, НЕ тип `MutSlice`): запись элемента write-through в буфер мастера;
  `push` НЕ запрещён, а реаллоцирует (cap==len) → detach (родитель не изменён).
- **Q-iter-mut** (NEW) — **ЗАКРЫТО: мутабельная итерация через `for mut x in v`** +
  `mut @iter()` (receiver-mut overload, НЕ имя `iter_mut`). Write-through в буфер;
  семантика как мут-view (write ок, рост→detach). Прецедент: Rust `iter_mut`, Kotlin —
  индексами. Согласуется с accessor receiver-mut конвенцией (Plan 135).
- **Q-vec-operator-plus** (NEW) — **ЗАКРЫТО: `a + b` = `@concat` (новый Vec).** `Vec[T]`
  реализует `@plus(other) -> Vec[T]` (как str `@plus`, D46), `a += b` ≡ `a.append(b)`.
  Не мутирует операнды (как Kotlin `+`, Python). Прецедент: Kotlin/Python/Ruby `+`.

**Документация (`docs/`):**
- `docs/vec.md` (NEW) — гайд: Vec/[]T, ленивые итераторы (vs eager), слайсы-views,
  sort/search, рецепты, таблица «откуда метод».
- `docs/vec-internals.md` (NEW, 153.0) — структура `collections/vec/`, RawMem-слой,
  layout `{data,len,cap}`.
- `docs/vec.md` раздел «слайсы» — модель `[]T`-view (cap==len, detach), split_at/chunks/windows (миграция не нужна — модель уже приземлена Plan 96).

---

## 5. Тесты (позитивные + негативные)

Фикстуры `nova_tests/plan153_N/`, через релизные `nova` + компилятор.

- **153.0:** папка-модуль `collections.vec` (co-equal файлы) компилируется; `vec.nv`+
  `vec_owned.nv` слиты, standalone-файлы удалены, импорты `vec_owned`→`vec`
  мигрированы (prelude + 5); `[]T` и `Vec[T]` взаимозаменяемы (POS); файл+папка `vec`
  одного имени → ambiguous (NEG-проверка, что не осталось); golden существующих
  Vec-тестов байт-в-байт.
- **153.1:** `swap`/`resize`/`cap(len())`-shrink (POS); `@cap(100)`→
  `@cap()==100` (ТОЧНО, round-trip), `with_capacity(100)`→точно 100, авто-рост:
  push в cap-8 на 9-м → `@cap()==16` (pow2), `@reserve(100)`→128 (амортизированный
  pow2); `@cap(n<len)`→panic (NEG); `resize`<0, `swap` OOB → panic (NEG).
- **153.2:** `iter().map().filter().collect()` без промежуточных аллокаций (POS, +
  проверка ленивости — побочный эффект считает только потреблённые); `sum`/`min`/
  `enumerate`/`zip` (POS); `zip` разных длин (NEG/усечение); пустой `min`→`None` (NEG).
- **153.3:** `sort`/`sort_by`/`binary_search`/`dedup`/`index_of`/`contains` (POS);
  `binary_search` на неотсортированном → unspecified-but-safe (NEG-doc); `sort` для
  `T` без `Compare` → compile-error (NEG).
- **153.4:** `v[a..b]` zero-copy (мутация владельца видна до realloc); **detach-on-
  resize:** `with_capacity(4)` точная → slice → push до realloc видит мутацию, push
  с realloc → slice = снимок старого буфера (не видит новые), GC-safe (POS);
  `split_at`/`chunks`/`windows` (POS); OOB slice → panic, `windows(0)` → panic/empty,
  push на `mut []T`-view (cap==len) → realloc/detach, родитель НЕ изменён (POS, Go-модель).
- **153.5:** `concat`/`flatten`/`rotate`/`drain` (POS); `drain` OOB → panic (NEG).
- **153.6:** `Vec[int]` как ключ `HashMap`/в `HashSet` (Hash, POS); `collect` в Vec
  (POS); `Vec[T]` без `Hash` как ключ → compile-error (NEG).
- **Регрессия:** полный `nova test` без новых FAIL на каждом sub-plan.

---

## 6. Критерии приёмки

**Глобальные (umbrella).**
- **G1.** Каждая «цель»-ячейка матрицы §1 закрыта реализацией или `[M-153-*]`-маркером.
- **G2.** Инварианты I1–I9 соблюдены (нет скрытого O(n); `[]T≡Vec`; view vs owned по
  `as_`/`to_`; один метод — один слой; протоколы консистентны; без циклов; fluent `@`).
- **G3.** Ленивые итераторы не хуже Rust/Kotlin-Sequence по набору адаптеров (или
  `[M-153-iter-*]` roadmap).
- **G4.** Полный `nova test` зелёный; все plan153*-фикстуры PASS.
- **G5.** Spec: D259–D264 + AMEND D239/D238/D240; Q-iterator-laziness/-slice-view/
  -vec-alias-completeness/-mutability закрыты записями; `docs/vec.md` +
  `vec-internals.md` + migration написаны.
- **G6.** Реализация структурирована (папка `collections/vec/` по слоям, без дубля
  `vec.nv`↔`vec_owned.nv`); координация с Plan 140.2 (bounds-элизия) соблюдена.

**Per-sub-plan A-критерии** — в файлах `153.N`. Ключевые:
- **153.0:** `[]T≡Vec` чистый алиас; папка-модуль `collections.vec` (co-equal файлы);
  `vec`+`vec_owned` слиты в один модуль, импорты мигрированы; ноль дублей; golden.
- **153.1:** swap/resize/cap-exact; capacity-инварианты держатся;
  `@cap(n)` realloc (n>=len, иначе panic); `@len(n)` запрещён; **fluent-цепочки
  работают** (`v.reserve(10).extend(xs).sort()`), `[M-138.2-vec-self-return]` закрыт;
  `append`/`extend` консолидированы в `append`; accessor-конвенция (D117 AMEND).
- **153.2:** ленивая цепочка без промежуточных аллокаций; collect; A-набор адаптеров.
- **153.3:** sort стабилен; binary_search корректен; dedup; bounds `T: Compare`.
- **153.4:** `v[a..b]` zero-copy view; split_at/chunks/windows на `[]T`-view; mut-view write-through; push→detach.
- **153.5:** concat/flatten/rotate/drain корректны.
- **153.6:** `Vec[T: Hash]` хешируем (HashSet/ключ); collect-target.

---

## 7. Для исполнителя (execution)

**Подготовка.**
- Постоянный worktree `nova-p153`. База — main (Vec-инфра в main: Plan 131/138).
- D-блоки **D259–D266** зарезервированы (резерв с запасом; занять D259–D264 + 2 на
  вырост). **D249–D258 — Plan 152, D256/D257 — Plan 140.2** (не трогать). Другие
  агенты — с **D267**. Решения по Q закрыты в §4 — перенеси записями в
  `spec/decisions/` + `spec/open-questions.md`.

**Parallel-safety.** Можно вести параллельно с **Plan 152** (str) и **Plan 140.2**
(Vec bounds). Точки координации в `compiler-codegen/src/codegen/emit_c.rs`:
**140.2** правит `Vec @index` codegen (lvalue/rvalue, bounds-элизия) — это пересекается
с 153.1/153.4 (Vec index/slice). **Сначала свериться с 140.2**, чтобы не переписать
встречно: 153 НЕ трогает bounds-элизию (I7), только добавляет методы/слайсы. 152 правит
**str**-index (другой регион). Конфликты `emit_c.rs` механические; второй мёрж чинит.

**Порядок и фазы.** 153.0 → 153.1 → 153.6 → (153.2-A ∥ 153.3 ∥ 153.4-A) → **B:**
153.2-B/153.4-B/153.5. Phase A обязательна сейчас; B — позже за `[M-153-*]`.

**Definition of Done на sub-plan:** реализация → spec (D + AMEND) + Q-записи →
доки (`vec.md`/`vec-internals.md`/migration) → pos **и** neg фикстуры
`nova_tests/plan153_N/` через **релизные** `nova`+компилятор → критерии файла + G1–G6
→ полный `nova test` без новых FAIL → коммит **пофазно**.

**Конвенции репо.** `git add` только конкретных файлов (никогда `-A`/`.`); перед
commit `git diff --cached --stat`; после крупной задачи — `project-creation.txt` +
`simplifications.md` + `nova-private/discussion-log.md`. Синтаксис Nova — только из
`spec/`+`examples/`.

**Фоновые агенты (если используются).**
- **НИКОГДА `git stash`** — repo-global, общий `.git` с конкурентными worktree →
  collision/потеря. Для baseline — **temp-worktree** или **commit-reset**, не stash.
- **Rate-limit устойчивость** — фоновые/workflow-агенты ловят серверный лимит и
  падают. Работай **идемпотентно/пофазно** (коммит-чекпойнт на каждой Ф.X →
  перезапуск без потери); не копи много несохранённого; дроби.
- **Изоляция** — каждый параллельный sub-plan в своём worktree (`nova-p153*`); не
  переключай ветки в чужом worktree; регистрируйся первой командой.

---

## Связанные D / Q-блоки

| D / Q | Связь |
|---|---|
| D259 (NEW) | Vec core API & capacity (153.1) |
| D260 (NEW) | ленивый итератор + адаптеры (153.2) |
| D261 (NEW) | sort & search (153.3) |
| D262 (NEW, минор) | slice-op surface на `[]T`-view (153.4) |
| D263 (NEW) ✅ | restructure-ops + оператор `+` (153.5) — записан в [10-overloading.md](../../spec/decisions/10-overloading.md) |
| D264 (NEW) | Vec-протоколы Hash + FromIterator (153.6) |
| D239 CONFIRM/AMEND | `[]T` чистый алиас завершён (153.0) |
| D238/D240 | `Index`/`MutIndex` — Vec index + Range-view |
| D241/D242 | `Next`/`Iter` — VecIter + ленивые адаптеры |
| D135 | cost-transparency (no hidden O(n)) |
| Q-iterator-laziness (NEW) | **ЗАКРЫТО** — ленивые адаптеры канон |
| Q-slice-view | **УЖЕ РЕШЕНО** (D238+Plan 96) — `v[a..b]`=`[]T` zero-copy view, cap==len; НЕ отдельный тип |
| Q-vec-alias-completeness (NEW) | **ЗАКРЫТО** — `[]T ≡ Vec[T]` чистый |
| Q-vec-mutability-through-view | мут-view `mut []T` (receiver-mut); push→realloc→detach |
| Q-iter-mut (NEW) | **ЗАКРЫТО** — `for mut x` / `mut @iter()`, write-through |
| Q-vec-operator-plus (NEW) | **✅ ЗАКРЫТО** (153.5) — `a+b`=`@plus`≡`@concat` (новый Vec), `+=`=`a=a+b`; запись в [open-questions.md](../../spec/open-questions.md) |

> Координация: **Plan 140.2** владеет bounds-элизией `v[i]` (НЕ дублировать).
> **Plan 152** — str-линза `as_bytes()` = `ro []u8` = `Vec[u8]`-view (общая slice-инфра).
