---
source_rev: 21dff1b37
source_date: 2026-08-02
---

# Vec[T] — нативный динамический массив Nova

[English](vec-owned.md) | **Русский**

`Vec[T]` — generic-растущий массив, реализованный целиком на Nova поверх
аллокации сырых указателей (`RawMem.alloc`). Доступен как
`std.collections.vec_owned.Vec`.

## Когда использовать Vec[T]

Используй `[]T` (встроенный срез) по умолчанию. Переходи на `Vec[T]`, когда:

- Тип элемента `T` — **value-struct** — `Option[U]`, именованный кортеж или
  `value`-record — чьё in-memory представление шире 8 байт (int64-слота,
  используемого внутренней erasure-моделью `[]T`).
- Нужно **типизированное хранилище**, где каждый элемент лежит в своём
  реальном C-типе без боксинга.

Для примитивов и heap-record типов `[]T` и `Vec[T]` на практике ведут себя
идентично; для краткости предпочитай `[]T`.

## Быстрый старт

```nova
import std.collections.vec_owned.{Vec}

fn main() -> () {
    // Build a vector from a literal element list (D259: `of`, not `from([…])`)
    mut v = Vec[int].of(10, 20, 30)
    assert(v.len() == 3)

    // Push and pop
    v.push(40)
    assert(v.pop() == Some(40))

    // Index access
    assert(v.get(0) == Some(10))
    assert(v.get(99) == None)

    // Iteration
    for x in v {
        println(x)    // 10, 20, 30
    }

    // Value-struct elements work correctly
    mut opts = Vec[Option[int]].new()
    opts.push(Some(1))
    opts.push(None)
    opts.push(Some(3))
    assert(opts.get(1) == Some(None))
}
```

## Конструкция

| Вызов | Результат |
|------|--------|
| `Vec[T].new()` | Пустой вектор, cap = 0, без аллокации |
| `Vec[T].with_capacity(n)` | Пустой вектор, преаллоцировано `n` слотов под элементы |
| `Vec[T].of(a, b, c)` | Вектор из **литерального списка элементов** (variadic) — **одна** аллокация |
| `existing.clone()` | **Конвертация** (deep-copy) существующей коллекции / `[]T` |

### `of` vs `.clone()` — что когда использовать (Plan 153.1 / [D259](../../spec/decisions/02-types.md#d259-конструктор-конвенция-vect--of-для-литерала-from-для-конверсии-plan-1531))

> **`Vec[T].from(coll)` RETRACTED (2026-07-20, Plan 200 П16)** — это был ровно
> `coll.clone()` (глубокая, поэлементная копия через `Clone`, D230), так что
> выделенный статический конструктор был избыточен. Используй `.clone()`
> напрямую.

Две различные роли — не путай их:

- **Построение из литерального списка элементов → `Vec[T].of(a, b, c)`**
  (variadic). Как `vec![a, b, c]` в Rust. Берёт элементы напрямую:
  **одна аллокация**.
- **Конвертация существующей коллекции → `existing.clone()`**. Как
  `iter.clone()` в Rust — глубокая, независимая копия того, что у тебя уже есть.

```nova
ro a = Vec[int].of(1, 2, 3)        // ✅ literal list → of (1 allocation)
ro b = Vec[int].new()              // ✅ empty
ro c = other_vec.clone()           // ✅ convert (deep-copy) an existing collection
```

**Почему литерал никогда не идёт по конверсионному пути.** Согласно
[D239](../../spec/decisions/02-types.md#d239-t--синтаксический-псевдоним-vect)
массив-литерал `[1, 2, 3]` *уже* является `Vec[int]` — одна аллокация на самом
литерале. Оборачивание его во второй вызов конструктора (старый антипаттерн
`from([1, 2, 3])`) скопировало бы его во **второй** буфер, суммарно две
аллокации, против **одной** у `Vec[int].of(1, 2, 3)`. Прибереги `.clone()` для
конвертации коллекции, которая у тебя уже есть.

> Когда тип элемента уже зафиксирован контекстом, конструктор даже не нужен —
> голый литерал `[a, b, c]` *и есть* `Vec[T]` (D239). `of` — только для
> inline-аннотации типа (позиция возврата, generic-контекст); `.clone()` —
> только для конвертации коллекции, которую ты уже держишь.

## Справочник методов

### Размер и ёмкость

| Метод | Сигнатура | Описание |
|--------|-----------|-------------|
| `len` | `@len() -> int` | Число живых элементов |
| `cap` | `@cap() -> int` | Число аллоцированных слотов |
| `is_empty` | `@is_empty() -> bool` | Истина, когда `len == 0` |
| `reserve` | `mut @reserve(additional int) -> ()` | Гарантировать место под `additional` элементов |
| `shrink_to_fit` | `mut @shrink_to_fit() -> ()` | Уменьшить ёмкость ровно до `len` |
| `shrink_to` | `mut @shrink_to(min_cap int) -> ()` | Уменьшить ёмкость до `max(len, min_cap)` |

### Добавление и удаление элементов

| Метод | Сигнатура | Описание |
|--------|-----------|-------------|
| `push` | `mut @push(v T) -> ()` | Добавить в конец; растёт ×2 при необходимости |
| `pop` | `mut @pop() -> Option[T]` | Удалить и вернуть последний элемент |
| `insert` | `mut @insert(i int, v T) -> ()` | Вставить по индексу `i`, сдвигая вправо; panic при `i > len` |
| `remove` | `mut @remove(i int) -> T` | Удалить по `i`, сдвигая влево; panic вне границ |
| `swap_remove` | `mut @swap_remove(i int) -> T` | O(1)-удаление: обмен с последним, затем pop; не сохраняет порядок |
| `clear` | `mut @clear() -> ()` | Установить `len = 0`, буфер сохраняется |
| `truncate` | `mut @truncate(n int) -> ()` | Укоротить до `n` элементов; no-op при `n >= len` |

### Доступ

| Метод | Сигнатура | Описание |
|--------|-----------|-------------|
| `get` | `@get(i int) -> Option[T]` | Элемент по индексу, с проверкой границ |
| `get_mut` | `mut @get_mut(i int) -> Option[*mut T]` | Сырой указатель на слот; валиден до следующего realloc |
| `first` | `@first() -> Option[T]` | Первый элемент |
| `last` | `@last() -> Option[T]` | Последний элемент |

### Массовые операции

| Метод | Сигнатура | Описание |
|--------|-----------|-------------|
| `extend` | `mut @extend(items []T) -> ()` | Добавить все элементы из среза `[]T` |
| `append` | `mut @append(mut other Vec[T]) -> ()` | Перенести всё из `other` в конец; `other` становится пустым |
| `retain` | `mut @retain(pred fn(T) -> bool) -> ()` | Оставить только элементы, где `pred` возвращает true; O(n) |
| `reverse` | `mut @reverse() -> ()` | Развернуть живые элементы на месте |

### Срезы и представления (Plan 153.4 / D262)

Zero-copy `[]T`-представления **того же типа**, разделяющие родительский буфер
(`cap == len`); типа `Slice` нет. Мутация с realloc на представлении отвязывает
его (модель Go, GC-safe); *владеющая* копия — `clone()`/`to_vec()`. Ленивые
срезовые итераторы `chunks`/`chunks_exact`/`rchunks`/`windows` живут в явно
импортируемом ленивом модуле (`import std.collections.vec_lazy`) — они отдают
`[]T`-представления по одному **без внешней аллокации `Vec`** (`slice::chunks`/
`windows` в Rust). См.
[`vec-internals.md` → Slices & views](../dev/vec-internals.md#slices--views-plan-1534--d262).

| Метод | Сигнатура | Описание |
|--------|-----------|-------------|
| `as_slice` | `@as_slice() -> []T` | Read-only zero-copy целое представление (НЕ копия) |
| `as_slice` (mut) | `mut @as_slice() -> mut []T` | Записывающее целое представление (recv-mut overload, как `mut @as_ptr`) |
| `split_at` | `@split_at(i int) -> ([]T, []T)` | Два смежных представления по `i`; `requires 0 <= i <= len` (OOB panic) |
| `split_first` | `@split_first() -> Option[(T, []T)]` | Первый элемент + хвостовое представление; пустой → `None` |
| `split_last` | `@split_last() -> Option[(T, []T)]` | Последний элемент + головное представление; пустой → `None` |
| `first_n` | `@first_n(n int) -> []T` | Префиксное представление; **кламп** (`n > len` → всё, `n <= 0` → пусто) |
| `last_n` | `@last_n(n int) -> []T` | Суффиксное представление; **кламп** (как `first_n`) |

**Ленивые срезовые итераторы** (`import std.collections.vec_lazy`; каждый
`-> BoxIter[[]T]`, `requires n > 0`):

| Метод | Сигнатура | Описание |
|--------|-----------|-------------|
| `chunks` | `@chunks(n int) -> BoxIter[[]T]` | Непересекающиеся куски по `n`; последний кусок короткий |
| `chunks_exact` | `@chunks_exact(n int) -> BoxIter[[]T]` | Только полные куски по `n`; короткий хвост отбрасывается |
| `rchunks` | `@rchunks(n int) -> BoxIter[[]T]` | Непересекающиеся куски с конца (отдаются сзади-наперёд); ведущий кусок короткий |
| `windows` | `@windows(n int) -> BoxIter[[]T]` | Пересекающиеся представления ширины `n` (`n-1` общих); `n > len` → пусто |

### Конверсия

| Метод | Сигнатура | Описание |
|--------|-----------|-------------|
| `iter` | `@iter() -> VecIter[T]` | Итератор-курсор по индексу |

### Протоколы

| Протокол | Метод | Примечания |
|----------|--------|-------|
| `Iterable[T]` | `@iter() / VecIter[T].@next()` | Синтаксис `for x in v` |
| `Equal` | `@equal(other Vec[T]) -> bool` | Поэлементно, через сравнение `as_slice` |
| `Clone` | `@clone() -> Vec[T]` | Аллоцирует новый буфер, копирует все элементы |
| `Display` | `@display(mut sb StringBuilder) -> ()` | Формат: `Vec[e0, e1, ..., eN-1]` |
| `Debug` | `@debug(mut sb StringBuilder) -> ()` | Тот же формат, для `${v:?}` |

## Примеры

### Рост и итерация

```nova
mut v = Vec[int].new()
for i in 0..10 { v.push(i) }
assert(v.len() == 10)
for x in v { print("${x} ") }    // 0 1 2 3 ... 9
```

### Вставка и удаление

```nova
mut v = Vec[int].of(1, 2, 4, 5)
v.insert(2, 3)                     // [1, 2, 3, 4, 5]
assert(v.remove(0) == 1)           // [2, 3, 4, 5]
assert(v.swap_remove(0) == 2)      // [5, 3, 4] (order disrupted)
```

### Фильтр через retain

```nova
mut v = Vec[int].of(1, 2, 3, 4, 5, 6)
v.retain(|x| x % 2 == 0)
assert(v.as_slice() == [2, 4, 6])
```

### Срезы и представления (zero-copy)

```nova
ro v = Vec[int].of(1, 2, 3, 4, 5)

// split_at: two views of the same buffer (contract 0 <= i <= len)
ro (l, r) = v.split_at(2)
assert(l.len() == 2 && r.len() == 3 && l[0] == 1 && r[0] == 3)

// first_n / last_n clamp ("take up to N")
assert(v.first_n(3).len() == 3)
assert(v.first_n(99).len() == 5)        // clamped to len
assert(v.last_n(2).equal(Vec[int].of(4, 5)))

// mut @as_slice writes through to the parent (until detach)
mut w = Vec[int].of(1, 2, 3)
mut s = w.as_slice()
s[0] = 99
assert(w[0] == 99)

// detach-on-resize: pushing onto a cap==len view reallocs; parent untouched
mut head = w.first_n(2)
head.push(7)                            // detaches into a fresh buffer
assert(w.equal(Vec[int].of(99, 2, 3)))  // parent unchanged
```

### Value-struct элементы

```nova
// Option[int] is a value-struct. []Option[int] would erase it.
// Vec[Option[int]] stores each NovaOpt_nova_int struct inline.
mut v = Vec[Option[int]].new()
v.push(Some(42))
v.push(None)
assert(v.get(0) == Some(Some(42)))
assert(v.get(1) == Some(None))
```

### Управление ёмкостью

```nova
mut v = Vec[int].with_capacity(100)
assert(v.cap() >= 100)
for i in 0..50 { v.push(i) }
v.shrink_to_fit()
assert(v.cap() == 50)
```

### Клон и равенство

```nova
ro a = Vec[int].of(1, 2, 3)
mut b = a.clone()
b.push(4)
assert(a.len() == 3)          // original unchanged
assert(b.len() == 4)
assert(a.equals(Vec[int].of(1, 2, 3)))
```

### Небезопасный get_mut

`get_mut` возвращает сырой мутабельный указатель для обновления на месте без
копирования:

```nova
mut v = Vec[int].of(10, 20, 30)
if Some(p) = v.get_mut(1) {
    unsafe { *p = 99 }
}
assert(v.get(1) == Some(99))
```

Примечание: указатель инвалидируется любым последующим `push`, `insert`,
`reserve` или другим realloc-способным вызовом.

## Сравнение с []T

| | `[]T` | `Vec[T]` |
|---|-------|----------|
| Выбор по умолчанию | Да | Нет |
| Примитивные элементы | Полная типизация | Полная типизация |
| Record-элементы | Указатель-в-слоте | Указатель-в-слоте |
| Элементы `Option[U]` | int64-erasure (сломано) | Inline-struct (правильно) |
| Элементы именованных кортежей | int64-erasure (сломано) | Inline-struct (правильно) |
| Value-record элементы | int64-erasure (сломано) | Inline-struct (правильно) |
| Итерация `for x in` | Встроенная | Через VecIter |
| Компилятор-магия | Да (NOVA_ARRAY_DECL) | Нет (чистый Nova) |
| Синтаксис литерала `[1,2,3]` | Да (`[1,2,3]` *это* `Vec` по D239) | Да — `[1,2,3]`, или `Vec[T].of(1,2,3)` для inline-типа (НЕ `from([…])`, D259) |

## Заметки о производительности

- `push`: амортизированный O(1). Начальная ёмкость 8; удваивается на каждом
  realloc.
- `get` / `get_mut`: O(1) арифметика указателей.
- `insert` / `remove`: O(n) сдвиг элементов — предпочитай `swap_remove`, когда
  порядок не важен.
- `as_slice`: O(n) копия — избегай на горячих путях; итерируй напрямую через
  `for x in v`.
- Аллокация буфера использует `RawMem.alloc` (Boehm GC-tracked, обнулённая,
  выравнивание 8 байт). GC консервативно сканирует буфер, так что указатели
  на элементы внутри буфера держат свои цели живыми.

## Ссылки на спеку

- [D231](../../spec/decisions/02-types.md#d231-rawmem-allocator-api--nova_alloc--nova_alloc_uncollectable--nova_free_uncollectable) — API аллокатора RawMem.
- [D232](../../spec/decisions/02-types.md#d232-vect--nova-native-generic-growable-array) — формальная спека Vec[T].
- [D216 §6](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — codegen арифметики указателей.
- [Q-vec-vs-slice](../../spec/open-questions.md#q-vec-vs-slice----vect-vs-t-which-to-use) — гайд принятия решения.
