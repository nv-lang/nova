<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 153 (umbrella) — Production-grade `Vec[T]` / `[]T`: API-паритет, итераторы, слайсы

> **Создан:** 2026-06-13.  **Статус:** 📋 **PLANNED umbrella**, P1.
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
> `a.max(b)`, как `a.sin()`) — **сейчас отсутствуют** (`[M-153-scalar-min-max]`);
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

### 153.3 — Sort & search `[D261, A]`
`@sort()`/`@sort_by(cmp)`/`@sort_by_key(key)` (stable; + `@sort_unstable*`),
`@is_sorted`/`@is_sorted_by`, `@binary_search(x)`/`@binary_search_by`/
`@binary_search_by_key`, `@contains`, `@index_of`/`@position(pred)`/`@rposition`,
`@dedup`/`@dedup_by`/`@dedup_by_key`, `@partition(pred)` (in-place split). Bounds на
`T: Compare` где нужно. **Roadmap:** `@select_nth_unstable` (quickselect, `[M-153-select-nth]`).
Эстимат ~2–3 dd.

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

### 153.5 — Restructure-ops + оператор `+` `[D263, Q-vec-operator-plus, B]`
`@concat(other) -> Vec[T]` + **оператор `+`** (`@plus`, `a + b` = новый Vec, как
str `@plus`; Q-vec-operator-plus), `[][]T.flatten()`, `@rotate_left(n)`/
`@rotate_right(n)`, `@drain(range) -> Vec[T]` (вырезать диапазон, вернуть владеемый),
`@insert_slice(i, sl)`. (extend/append/retain/splice — уже есть, аудит.) Эстимат ~1.5–2 dd.

### 153.6 — Протоколы (Equal/Compare/Clone/**Hash**/Display/Debug/FromIterator) `[D264, A]`
Добавить `Vec[T: Hash] @hash()` (для `HashSet[Vec]`/ключа); закрепить `FromIterator`/
`collect`-target (мост с 153.2); аудит consistency Equal/Compare/Clone/Display/Debug
(уже есть). Эстимат ~1.5 dd.

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
- **D262** (NEW, минорный) — slice-op surface (split_at/chunks/windows) на `[]T`-view модели D238/Plan 96 (БЕЗ новых типов; подтверждает single-type).
- **D263** (NEW) — restructure-ops (concat/flatten/rotate/drain).
- **D264** (NEW) — Vec-протоколы (`Hash` + FromIterator/collect).
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
| D263 (NEW) | restructure-ops (153.5) |
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
| Q-vec-operator-plus (NEW) | **ЗАКРЫТО** — `a+b`=`@concat`, `+=`=`append` |

> Координация: **Plan 140.2** владеет bounds-элизией `v[i]` (НЕ дублировать).
> **Plan 152** — str-линза `as_bytes()` = `ro []u8` = `Vec[u8]`-view (общая slice-инфра).
