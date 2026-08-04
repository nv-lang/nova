---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Ленивые итераторы над `Vec[T]` / `[]T`

[English](vec-lazy.md) | **Русский**

> **Аудитория:** пользователи Nova. **Спека:** [D260](../../spec/decisions/02-types.md#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532)
> (модель ленивого итератора), [D277](../../spec/decisions/02-types.md#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2)
> (by-value `BoxIter` + zero-cost `vec_iter_zc`), [D239](../../spec/decisions/02-types.md#d239-t--синтаксический-псевдоним-vect)
> (`[]T ≡ Vec[T]`). **Интерналы:** [`vec-internals.md`](../dev/vec-internals.md). Plan 153.2.

Ленивый итератор обрабатывает вектор **по одному элементу за раз, по
требованию**, **без промежуточных аллокаций**. Построение пайплайна не делает
никакой работы; только *терминатор* протягивает элементы через него, и он тянет
ровно столько, сколько нужно.

```nova
import std.collections.vec_lazy

ro v = Vec[int].of(1, 2, 3, 4, 5, 6)
ro got = v.lazy().map(|x| x * 10).filter(|x| x > 25).collect()
assert(got == [30, 40, 50, 60])
```

## Начало работы

Ленивый слой — **опт-ин** модуль — импортируй его явно:

```nova
import std.collections.vec_lazy
```

(Его *нет* в prelude: ленивые адаптеры принимают замыкания, а prelude-глобальный
метод с замыканиями утёк бы своими generics/params в каждый юнит — см.
[`vec-internals.md`](../dev/vec-internals.md). Жадные комбинаторы
`collections.vec_seq` ограничены так же.)

Каждый пайплайн начинается с `v.lazy()`, превращающего `Vec[T]` (или любой
`[]T`, поскольку это один тип — D239) в `BoxIter[T]`:

```
v.lazy()  →  BoxIter[T]   →  .map(..) .filter(..) ...   →  terminator
  ^entry        cursor          ^adapters (lazy)             ^drives the chain
```

## Ленивость vs жадность — почему это важно

| | Eager (`collections.vec_seq`) | Lazy (`collections.vec_lazy`) |
|---|---|---|
| `v.map(f).filter(p)` | строит новый `Vec` **на каждом шаге** (O(n) аллокаций) | строит ноль `Vec`; оборачивает замыкания |
| Сделанная работа | всегда обрабатывает **все** элементы на каждом шаге | только то, что тянет терминатор |
| Short-circuit | нет — полная материализация | да — `find`/`any`/`all`/`take`/`nth` останавливаются рано |
| Результат | `Vec` после каждого адаптера | значение/`Vec` только на терминаторе |

Ленивость — это **канонический, безаллокационный** путь
([Q-iterator-laziness](../../spec/open-questions.md)). Жадные комбинаторы
`vec_seq` сохранены как переходная поверхность; бери `lazy()`, когда чейнишь
больше одного шага или хочешь short-circuit.

### Ленивость, продемонстрированная

Ничего не выполняется, пока терминатор не заведёт цепочку, и трогаются только
вытянутые элементы:

```nova
ro v = Vec[int].of(1, 2, 3, 4, 5)

// No terminator → no work. `map` never runs.
ro _pipeline = v.lazy().map(|x| x * 2).filter(|x| x > 0)

// `take(3)` pulls exactly 3 source elements — `map` runs 3 times, not 5.
ro first3 = v.lazy().map(|x| x * 10).take(3).collect()   // [10, 20, 30]

// `find` short-circuits at the first match.
ro hit = v.lazy().map(|x| x).find(|x| x == 3)            // Some(3), map ran 3×
```

## API — фаза A

### Вход

| Метод | Возвращает | Примечания |
|---|---|---|
| `v.lazy()` | `BoxIter[T]` | начать ленивый пайплайн над вектором / срезом |

### Адаптеры (ленивые — возвращают новый `BoxIter`, без аллокации)

| Адаптер | Сигнатура | Отдаёт |
|---|---|---|
| `map` | `@map[U](f fn(T) -> U) -> BoxIter[U]` | `f(x)` для каждого элемента |
| `filter` | `@filter(pred fn(T) -> bool) -> BoxIter[T]` | элементы, где `pred` истинно |
| `filter_map` | `@filter_map[U](f fn(T) -> Option[U]) -> BoxIter[U]` | `Some(u)` из `f`; `None` пропускается |
| `enumerate` | `@enumerate() -> BoxIter[(int, T)]` | пары `(index, x)` |
| `take` | `@take(n int) -> BoxIter[T]` | не более первых `n` |
| `skip` | `@skip(n int) -> BoxIter[T]` | все кроме первых `n` |

### Срезовые итераторы (ленивые продюсеры `[]T`-представлений — Plan 153.4)

Инстанс-методы на `Vec[T]` (не на `BoxIter`), разбивающие вектор в ленивый
итератор **zero-copy `[]T`-представлений** (заголовков `Vec[T]` с `cap == len`,
разделяющих родительский буфер как `v[a..b]`). Внешний `Vec[Vec[T]]` заранее не
аллоцируется — каждый pull `step` строит одно представление поддиапазона по
требованию (`slice::chunks`/`windows` в Rust). Каждый `requires n > 0` (нулевой/
отрицательный размер паникует, без клампа). Заводи их любым терминатором:
`v.chunks(n).collect()` материализует `[][]T` только по требованию,
`v.windows(n).map(|w| …)` / `.fold` / `.count` вообще никогда не аллоцирует
внешний `Vec`.

| Адаптер | Сигнатура | Отдаёт |
|---|---|---|
| `chunks` | `@chunks(n int) -> BoxIter[[]T]` | непересекающиеся куски по `n`; **последний кусок короткий** |
| `chunks_exact` | `@chunks_exact(n int) -> BoxIter[[]T]` | только полные куски по `n`; **короткий хвост отбрасывается** |
| `rchunks` | `@rchunks(n int) -> BoxIter[[]T]` | куски с конца (отдаются сзади-наперёд); **ведущий кусок короткий** |
| `windows` | `@windows(n int) -> BoxIter[[]T]` | пересекающиеся представления ширины `n` (`n-1` общих); `n > len` → пусто |

```nova
ro v = Vec[int].of(1, 2, 3, 4, 5)
assert(v.chunks(2).collect().len() == 3)             // [1,2] [3,4] [5]
assert(v.chunks_exact(2).collect().len() == 2)       // [1,2] [3,4] (drops [5])
assert(v.windows(2).collect().len() == 4)            // [1,2] [2,3] [3,4] [4,5]
// lazy — no Vec[Vec[int]] ever allocated:
ro pair_sums = v.windows(2).map(|w| w[0] + w[1]).collect()
assert(pair_sums == [3, 5, 7, 9])
```

### Терминаторы (заводят цепочку / short-circuit)

| Терминатор | Сигнатура | Результат |
|---|---|---|
| `collect` | `mut @collect() -> Vec[T]` | слить в свежий `Vec` (цель сбора по умолчанию) |
| `collect_set` | `[T Hash] mut @collect_set() -> Set[T]` | слить в `Set` (дедуп) |
| `fold` | `mut @fold[Acc](init Acc, f fn(Acc, T) -> Acc) -> Acc` | левая свёртка |
| `reduce` | `mut @reduce(f fn(T, T) -> T) -> Option[T]` | свёртка от первого; `None` если пусто |
| `count` | `mut @count() -> int` | число оставшихся элементов |
| `sum` | `mut @sum(zero T) -> T` | сумма, начиная с аддитивной единицы |
| `any` | `mut @any(pred fn(T) -> bool) -> bool` | `true` на первом совпадении (short-circuit) |
| `all` | `mut @all(pred fn(T) -> bool) -> bool` | `false` на первом промахе; вакуумно `true` |
| `find` | `mut @find(pred fn(T) -> bool) -> Option[T]` | первое совпадение или `None` |
| `for_each` | `mut @for_each(f fn(T) -> ()) -> ()` | прогнать `f` ради side-effect |
| `min` | `[T Compare] mut @min() -> Option[T]` | наименьший по `@compare`, или `None` |
| `max` | `[T Compare] mut @max() -> Option[T]` | наибольший по `@compare`, или `None` |
| `nth` | `mut @nth(n int) -> Option[T]` | 0-based `n`-й элемент или `None` |
| `last` | `mut @last() -> Option[T]` | последний элемент или `None` |

`sum(zero T)` принимает аддитивную единицу (`0` / `0.0`) явно, вместо того чтобы
полагаться на числовой протокол — так тип элемента и результат пустого
итератора однозначны.

## Рецепты

```nova
import std.collections.vec_lazy

// Transform then collect
ro doubled = v.lazy().map(|x| x * 2).collect()

// Filter then sum
ro total = v.lazy().filter(|x| x % 2 == 0).sum(0)

// Sum of squares of the odd elements
ro s = v.lazy().map(|x| x * x).filter(|x| x % 2 == 1).fold(0, |acc, x| acc + x)

// Window the middle: drop 2, keep 3
ro mid = v.lazy().skip(2).take(3).collect()

// filter_map: keep + transform in one pass
ro tripled3 = v.lazy()
    .filter_map(|x| if x % 3 == 0 { Some(x * 10) } else { None })
    .collect()

// enumerate: project index + value in the SAME stage (collapse the tuple with map)
ro projected = v.lazy().enumerate().map(|p| p.0 * 100 + p.1).collect()

// Short-circuiting search — stops at the first match
ro found = v.lazy().find(|x| x > 100)

// Bounded scan — only the first 3 are ever touched
ro early = v.lazy().map(|x| x + 1).take(3).any(|x| x == 3)
```

## FromIterator / collect-target (Plan 153.6, D264)

Материализуй пайплайн (или любой итератор-источник) в выбранную коллекцию.

```nova
import std.collections.vec_lazy
import std.collections.set.{Set}
import std.collections.hashmap.{HashMap}

// Default target — Vec
ro v = src.lazy().map(|x| x * 2).collect()

// Set target — dedup (Rust `iter.collect::<HashSet<_>>()`)
ro s = src.lazy().filter(|x| x > 0).collect_set()

// HashMap target — collect pairs, then `from`
ro m = HashMap[int, int].from(src.lazy().map(|x| (x, x * x)).collect())

// Set target (alternative) — collect a Vec, then `from_iter`
ro s2 = Set[int].from_iter(src.lazy().collect())

// Build a Vec from ANY Iter source directly (no lazy stage) — `@extend`
ro from_range = Vec[int].new().extend(0..5)        // [0, 1, 2, 3, 4]
ro from_vec   = Vec[int].new().extend(other_vec)   // copy
```

Nova типизирует итераторы **структурно** ([D58]): любой `mut @next() -> Option[T]`
итерируем, так что FromIterator — это *набор* конструкторов/терминаторов, а не
один принуждаемый single-method протокол — `@collect`/`@collect_set`
(терминаторы), `from`/`from_iter` (конструкторы из собранного `Vec`), `@extend`
(построение из источника). Загейчено (компиляторные пробелы, не упрощения):
*статический* generic-конструктор `Vec[T].from_iter[S Iter[T]]`
(`[M-153.6-collect-static-generic]` — используй `Vec[T].new().extend(src)`)
и терминатор `@collect_map()` для tuple-элементов
(`[M-153.6-collect-map-tuple-receiver]` — используй `HashMap.from(pairs.collect())`).

## Известные ограничения (фаза A)

- **`enumerate` затем tuple-сохраняющий адаптер.** `enumerate().map(|p| ...)`
  (где `map` поглощает кортеж `(int, T)` в той же стадии) поддерживается.
  Чейнинг tuple-СОХРАНЯЮЩЕГО адаптера прямо после `enumerate` —
  `enumerate().filter(..)` / `.take(n)` / `.skip(n)`, где элемент остаётся
  кортежем — загейчен на остаточный пробел типизации замыканий
  (`[M-153.2-tuple-elem-adapter]`); сначала схлопни кортеж через `map`.
- **Адаптеры фазы B ещё не присутствуют** (роадмап, не упрощение):
  `zip`/`unzip`/`chain`/`flat_map`/`flatten`/`scan`/`inspect`/`step_by`/
  `take_while`/`skip_while`/`peekable`/`min_by[_key]`/`max_by[_key]`/`partition`/
  `chunk_by`/`into_iter`, плюс мутабельная итерация (`for mut x` / `mut @iter()`).
  (`FromIterator`/collect-target сделан — см. D264.)
- **Цена.** `BoxIter[T]` теперь `value`-record (D277 Stage 1), так что **сам
  record-обёртка стоит ноль аллокаций в куче** — цепочка
  `v.lazy().map().filter().collect()` упала с 5 heap-боксов `BoxIter` до **0**,
  передаётся по значению на стеке. Что остаётся забоксированным в этой модели —
  пер-адаптерное **`step`-замыкание** (heap-thunk + бокс захваченного источника,
  плюс вызов `step()`-указателя на элемент). Для безаллокационной *и*
  безындирекционной цепочки используй zero-cost generic-over-source собрата —
  см. ниже.

## Zero-cost собрат — `collections.vec_iter_zc`

`vec_lazy`/`BoxIter` — это **closure-fluent** поверхность: один стёртый
тип-курсор, единый `BoxIter[T]` на каждой стадии, ценой забоксированного `step`
на адаптер. Для горячих путей есть **безаллокационный, безындирекционный**
модуль-собрат:

```nova
import std.collections.vec_iter_zc

ro v = Vec[int].of(1, 2, 3, 4, 5, 6)
ro got = v.ziter().zmap(|x| x * 10).zfilter(|x| x > 25).zcollect()
assert(got == [30, 40, 50, 60])
```

Каждый адаптер — свой **generic-over-source `value`-record** (`MapIter[I,T,U]` /
`FilterIter[I,T]` / `FilterMapIter[I,T,U]`), держащий апстрим-итератор **inline**
как поле `src I` — не бокс `step`-замыкания. `@next()` вызывает `(@src).next()`
статической, **мономорфизированной** диспетчеризацией, так что цепочка
`v.ziter().zmap(f).zfilter(p)` мономорфизируется в *один* вложенный конкретный
тип `FilterIter[MapIter[VecIter[int], int, int], int]`, и каждый `.next()`
инлайнится до базового `VecIter.next()` — без вызова function-указателя на
элемент.

Методы адаптер-на-адаптере пишут свой возвращаемый тип с **`Self`** в
*позиции вложенного* generic-аргумента — например,
`MapIter[I,T,U] @zmap(...) -> MapIter[Self,U,V]` (где `Self ≡ MapIter[I,T,U]`,
mono-получатель), и аналогично `-> FilterIter[Self,U]` / `-> FilterMapIter[Self,U,V]`.
Это опускает повторяющийся тип получателя в позиции повторного вложения;
семантика идентична полному написанию типа получателя. Поддержка компилятора для
`Self` как вложенного generic-аргумента (в возврате **и** в параметре) на
value-generic mono вышла 2026-06-15 — см.
[D66 → AMEND «Self как вложенный generic type-arg»](../../spec/decisions/02-types.md#d66-self-universal--ссылка-на-обобщающий-тип-в-методах-effects-protocols).
(Chain-ENTRY `VecIter[T] @zmap -> MapIter[Self,T,U]` **ещё** не покрыт —
single-param источник `VecIter[T]` остаётся явным; `[M-138.2-self-in-param]`.)

| | `vec_lazy` (`BoxIter`) | `vec_iter_zc` (Map/Filter) |
|---|---|---|
| record-обёртка на адаптер | 0 heap (by-value, D277 Stage 1) | 0 heap (by-value) |
| бокс источника (`_box_src`) на адаптер | 1 heap | **0** (источник держится inline) |
| thunk `step`-замыкания на адаптер | 1 heap (`NovaClosBase`) | **0** (статическая диспетчеризация) |
| пер-элементная индирекция источника | вызов fn-ptr | **нет** (инлайн) |
| окружение/бокс capture-free `f`/`pred` замыкания | 0 heap (D277 Stage 3 — статический синглтон) | 0 heap (статический синглтон) |
| тело терминатора (`collect_into`/`fold`/`sum`/…) | — | **0 `nova_alloc`** (D277 Stage 4) |
| остаточный heap | env *захватывающего* `f`/`pred` + курсор источника `VecIter` | то же |

Для канонической цепочки `map().filter().collect()` это убирает **6
адаптер-аллокаций и 9 боксов источника**. По состоянию на **Stage 3** D277
замыкание **без захватов** (частая форма `|x| x * 3`) тоже стоит **0 heap** —
оно эмитится как статический синглтон с областью видимости файла вместо env-бокса +
closure-бокса на каждый call-site (замерено: closure-аллокации `4 → 0` для
цепочки `.zmap().zfilter().zcollect()`, `6 → 0` с `.zfold()`). Единственный heap,
оставшийся для полностью capture-free цепочки, — курсор источника `VecIter`;
*захватывающее* замыкание всё ещё аллоцирует свой env на каждый инстанс
(неустранимо без замыканий-как-mono-типов — `[M-153.2-closure-as-mono-type]`),
а **сам вызов** всё ещё является fn-ptr индирекцией
(`[M-153.2-Z-closure-devirt]`).

**Сводка аллокаций** (каноническая цепочка над `Vec[int]`, замерено
в сгенерированном C):

| цепочка | boxed `vec_lazy` | zero-cost `vec_iter_zc` | + Stage 3 devirt (`vec_iter_zc`) |
|---|---|---|---|
| `.map(f).filter(p).collect()` (capture-free `f`/`p`) | wrapper + source + step + closure heap | source/step **0**; closure env **4** | closure env **0** (синглтон); только результат `Vec` |
| `.map(f).filter(p).collect_into(out)` | — | тело терминатора **0 `nova_alloc`** (Stage 4) | **0** + амортизированный **0** результат (переиспользует `out`) |
| `.map(f).filter(p).fold(0, g)` (capture-free) | closure heap | closure env **6** | closure env **0**; скалярный результат (**0**) |

Оба сосуществуют за отдельными явными импортами — `vec_iter_zc` **не** замена.
Бери его на горячих путях; `vec_lazy` остаётся эргономичным дефолтом с одним
курсором. Вход — `v.ziter()`; адаптеры `zmap`/`zfilter`/`zfilter_map`;
терминаторы `zcollect`/`zcollect_into`/`zfold`/`zcount`/`zsum`/`zfor_each`/
`zany`/`zall`/`zfind`. `take`/`skip`/`enumerate` (stateful / tuple-элементы)
пока остаются на boxed `vec_lazy`.

`zcollect_into(out)` — это **безаллокационный** приёмник (D277 Stage 4): он
сливает цепочку **добавлением** в переиспользуемый `Vec[T]` вызывающего вместо
аллокации свежего результата. Его мономорфизированное тело — `0 nova_alloc`.
Очисти буфер первым, чтобы использовать как свежий приёмник (`out.clear()`
сохраняет backing store, так что переиспользуемый `out` амортизируется до
**ноля** аллокаций):

```nova
mut out = Vec[int].new()
for batch in batches {
    out.clear()                                       // len=0, buffer kept
    batch.ziter().zmap(|x| x * 2).zfilter(|x| x > 0).zcollect_into(out)
    consume(out)                                      // reuse `out` next iteration
}
```

## См. также

- [`vec-internals.md`](../dev/vec-internals.md) — раскладка модулей, boxed-fluent
  форма, zero-cost generic-over-source собрат, Compare/Equal.
- [D260](../../spec/decisions/02-types.md#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532) — decision-record boxed-fluent.
- [D264](../../spec/decisions/02-types.md#d264-vec-протоколы-hash--fromiterator--collect-target-plan-1536) — Hash + FromIterator / collect-target.
- [D277](../../spec/decisions/02-types.md#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2) — by-value мономорфизация `BoxIter` + zero-cost собрат `vec_iter_zc`.
- [D58]: ../spec/decisions/03-syntax.md — структурная итерация `Iter`/`Next`.
- [Q-iterator-laziness](../../spec/open-questions.md) — почему ленивость — канон.

[D58]: ../spec/decisions/03-syntax.md
