# Эволюция решений Nova

История пересмотров: что менялось, почему, какие решения были отменены
или заменены последующими.

> **Зачем это нужно.** Дизайн языка — итеративный процесс. Решения
> уточняются, отменяются, заменяются. Чтобы будущая Claude или
> программист, читающий журнал, не ловил «противоречия» — здесь зафиксировано,
> где и когда что менялось.

## Главные пересмотры

### Память: `~T` / `~&T` → managed GC

**Что было:** опт-ин cycle collection. Программист выбирал префикс
для каждого типа: `~T` (acyclic), `~&T` (с cycle collector), `~weak`
(слабая ссылка). Эффект `Alloc[Cycle]`. Тип `Weak[T]` в stdlib.

**Что стало:** managed concurrent GC по умолчанию (как Go). Никаких
префиксов. Циклы освобождаются автоматически. Real-time зоны — через
`region { ... }` блок и эффект `Realtime`.

**Почему пересмотрели:**
- Целевая ниша Nova — backend, не embedded. Современный GC справляется.
- AI-first: LLM не должна выбирать префикс для каждой структуры.
- Когнитивный налог на программиста.
- Опыт Java/Swift/C++ — misuse weak-ссылок широко известен.

**Связанные D:**
- Старое: D21 (отменено).
- Новое: [D6](../05-memory.md#d6).

### `&T` borrow → не вводим

**Что было:** в первой редакции после перехода на managed GC я
(агент) предложил оставить `&T` как opt-in borrow для hot path.

**Что стало:** `&T` отменён полностью. Передача объекта = передача
указателя в managed heap. Escape analysis закрывает hot-path. Для
real-time — `region { ... }`.

**Почему отменили:**
- Скопировано рефлекторно из Rust, где borrow нужен из-за отсутствия GC.
- Slice `[]T` уже передаётся эффективно.
- Lifetime checker — research-уровень, выгода низкая для прикладного
  языка с GC.
- Прецедент Go — нет borrow, успешно работает.

**Связанные D:** [D6](../05-memory.md#d6).

### Realtime region: всегда явный → implicit для тела функции

**Что было:** функция с эффектом `Realtime` обязательно содержала
явный `region { ... }` блок вокруг тела. Дублировало контракт
(`Realtime` + `region`).

**Что стало:** компилятор оборачивает тело Realtime-функции в
implicit region автоматически. Явный `region { ... }` нужен только
для контроля над несколькими аренами.

**Почему пересмотрели:**
- Дублирование контракта.
- AI-friendly: LLM не должна угадывать, нужен ли `region`.

**Связанные D:** [D6](../05-memory.md#d6).

### Парадигма: `trait` + `impl` → `protocol`

**Что было:** Rust-style контракты — отдельный keyword `trait`, явный
блок `impl Trait for Type`. Параметр функции принимал `[T: Trait]`
bounds.

**Что стало:** keyword **`protocol`**. Структурное соответствие —
любой тип со совпадающими методами автоматически удовлетворяет
protocol'у. Никаких `impl`-блоков, никаких `[T: Bound]` bounds.

**Почему пересмотрели:**
- Структурный подход согласуется со «всё структурно» (Go-стиль).
- Меньше синтаксиса.
- AI-first: меньше способов выразить «это интерфейс».
- Эффекты в сигнатурах методов делают структурный тип строже Go-interface
  — это уникальное свойство Nova.

**Связанные D:**
- Старое: D1 (формулировка обновлена), D15 (revised).
- Новое: [D9 / D15](../02-types.md#d15) — структурный механизм,
  [D42](../02-types.md#d42) — keyword `protocol`.

### Эффекты: lowercase `throws io async` → PascalCase `Throws Io Async`

**Что было:** эффекты как keyword'ы lowercase: `fn save(u: User)
throws io async -> ()`.

**Что стало:** эффекты — обычные **типы** в PascalCase. Тот же
синтаксис, что для всех типов в Nova.

**Почему пересмотрели:**
- Унификация: эффекты и типы — одно понятие ([D18](../04-effects.md#d18)).
- Согласованность с правилами именования ([D30](../03-syntax.md#d30)).

**Связанные D:** [D2](../04-effects.md#d2), [D3](../04-effects.md#d3),
[D11](../04-effects.md#d11), [D18](../04-effects.md#d18),
[D30](../03-syntax.md#d30).

### `effect X { ... }` keyword → обычный `type X { ... }`

**Что было:** объявление эффекта через специальный keyword `effect`:

```nova
effect Logger { log(msg str) -> () }
```

**Что стало:** эффект — обычный `type` с операциями:

```nova
type Logger { log(msg str) -> () }
```

**Почему пересмотрели:**
- Унификация: эффекты — частный случай типов.
- Меньше keyword'ов.

**Связанные D:** [D18](../04-effects.md#d18).

### `handler` keyword → handler-литерал и обычные функции

**Что было:** специальный синтаксис объявления handler'а:

```nova
handler json_logger Logger {
    log(msg) => println("[LOG] ${msg}")
}
```

**Что стало:** handler — обычное **значение**, литерал по форме
record-литерала или handler-лямбда:

```nova
let json_logger = Logger {
    log(msg) => resume(println(msg))
}

with Logger = json_logger { ... }

// или handler-лямбда (для эффектов с одной операцией)
with Throws[Error] = (err) => Log.warn("op failed: ${err}") {
    Db.exec(...)
}
```

**Почему пересмотрели:**
- Handler — это значение, не отдельная категория.
- Структурное единство — handler-литерал симметричен record-литералу.

**Связанные D:** [D11](../04-effects.md#d11), [D31](../04-effects.md#d31).

### Match-arms: `->` → `=>`

**Что было:**

```nova
match x {
    Some(v) -> v * 2
    None    -> 0
}
```

**Что стало:**

```nova
match x {
    Some(v) => v * 2
    None    => 0
}
```

**Почему пересмотрели:**
- `->` уже занято для возвращаемого типа функции.
- `=>` стандарт в современных языках (C# / F# / Scala 3).
- Унификация с handler-литералами и лямбдами.

**Связанные D:** [D19](../03-syntax.md#d19),
[D22](../03-syntax.md#d22).

### Тело функции: `=` → `=>`

**Что было:** тело функции через `=`:

```nova
fn double(x: int) -> int = x * 2
```

**Что стало:** через `=>`:

```nova
fn double(x int) -> int => x * 2
```

**Почему пересмотрели:**
- `=` ассоциируется с присваиванием, путает.
- Унификация с лямбдами и match-arms (везде `=>`).

**Связанные D:** [D20](../03-syntax.md#d20),
[D22](../03-syntax.md#d22).

### Объявление типа: `type X = { поля }` → `type X { поля }`

**Что было:** record-тип объявлялся со знаком равенства:

```nova
type User = { id u64, name str }
```

**Что стало:** без `=`:

```nova
type User { id u64, name str }
```

**Почему пересмотрели:**
- `=` означает «справа выражение типа». Когда справа форма данных
  (`{...}` или `(...)`) — `=` лишний.

**Связанные D:** [D17](../02-types.md#d17).

### Структурный тип: `type X = { методы }` → `protocol X { методы }`

**Что было:** структурный интерфейс — alias на type-выражение со
скобками методов:

```nova
type Hashable = {
    hash() -> u64
    eq(other Self) -> bool
}
```

**Что стало:** отдельный keyword `protocol`:

```nova
protocol Hashable {
    hash() -> u64
    eq(other Self) -> bool
}
```

**Почему пересмотрели:**
- `type X = { методы }` визуально путалось с `type X { поля }`.
- `protocol` явно сигнализирует «это контракт, не данные».
- Соответствует Swift / Python `typing.Protocol`.

**Связанные D:** [D42](../02-types.md#d42).

### Видимость: `pub` → `export`

**Что было:** Rust-style `pub fn`.

**Что стало:** `export fn`.

**Почему пересмотрели:**
- Симметрия с `import`.
- Освобождает `use` для embed/delegation в `type`.
- AI-friendly — слово длиннее, но смысл прозрачнее.

**Связанные D:** [D5](../07-modules.md#d5),
[D29](../07-modules.md#d29).

### Методы: `mut self` → `mut @method`

**Что было:** метод инстанса — функция с явным `self` параметром:

```nova
fn Account.deposit(mut self, amount money) throws -> ()
```

**Что стало:** `@`-синтаксис для методов:

```nova
fn Account mut @deposit(amount money) Throws -> ()
```

**Почему пересмотрели:**
- Короче.
- `@field` для доступа к self-полям без `self.` префикса.
- Единый паттерн `fn Type @method` / `fn Type mut @method`.

**Связанные D:** [D35](../03-syntax.md#d35).

### Поля типа: per-field `mut`/`final`/`readonly` → mut по умолчанию + `readonly`

**Что было (D32 первоначально):** каждое поле имело явный модификатор —
`final` для never-mut, `let` или ничего для mut. Получалось много
шума (~18 `mut` в большом record).

**Что стало (D36):** поля по умолчанию mutable у `mut`-binding'а
(передача через `mut acc`), `readonly` для never-mut, `mut` остался
для cache/lazy-полей.

**Почему пересмотрели:**
- Реальный пример (`RunAcc` из oxsar-port) показал, что 18 `mut`
  делают код нечитаемым.
- Большинство полей в record мутируются вместе со структурой —
  логично делать mut по умолчанию.

**Связанные D:** [D32](../02-types.md#d32) (для параметров),
[D36](../02-types.md#d36) (для полей — пересмотрел D32).

### Возврат-тип в expression-body: всегда обязателен → опционален

**Что было:** `-> T` обязателен везде, даже для тривиальных
expression-body функций.

**Что стало:** в expression-body (`=> expr`) `-> T` опционален —
тип выводится из тела. В block-body (`{ ... }`) обязателен (если не
unit). Style guide рекомендует явный `-> T` для `export`-функций.

**Почему пересмотрели:**
- Простые геттеры (`fn @len() => @count`) выглядели многословно.
- Inference тривиален для одного выражения, локальный.

**Связанные D:** [D32 / D45](../03-syntax.md#d45).

### Operator overloading: запрещено → через `@`-методы

**Что было:** в раннем дизайне overloading намекался как «только для
стандартных traits, не для custom-типов».

**Что стало:** перегрузка через имена методов (`@plus` для `+`,
`@eq` для `==` и т.д.). Custom-операторы запрещены, фиксированный
mapping.

**Почему пересмотрели:**
- Math-типы (`Duration`, `money`, `Vector`) требуют арифметики —
  иначе цепочки `.plus().times()` нечитаемы.
- Bitflags закрывает Q16 через newtype с `@or`/`@and`.

**Связанные D:** [D46](../03-syntax.md#d46).

### Tagged template literals: де-факто → формализовано

**Что было:** `json\`{}\`` использовался в примерах, но грамматика
не зафиксирована.

**Что стало:** D48 фиксирует — tagged template — обычная функция со
специальной сигнатурой (parts + args).

**Связанные D:** [D48](../03-syntax.md#d48).

## Открытые сюжеты

Несколько решений всё ещё могут эволюционировать:

- **Default методы протоколов** — пока запрещены, могут быть введены.
- **Generic bounds** (`HashMap[K: Hashable, V]`) — нужны для
  type-safety, нужно отдельное D-решение.
- **Per-field видимость** — сейчас MVP-компромисс (все поля
  публичны), может расшириться.
- **Effect-aware SMT** — частичная поддержка в v1.0, полная —
  research.
- **Макросы / `comptime`** — открытый вопрос.

### Объявление типов revised: D17 → D52

**Что было:** D17 фиксировал систему «один разделитель списка —
запятая, `=` ставится только когда справа выражение типа»:

```nova
type UserId = u64                       // alias через =
type Color = Red, Green, Blue            // sum через = и ,
type User { id u64, name str }           // record без =
```

Newtype как явная фича отсутствовал; domain-типы делались через
record-обёртку (`type UserId { value u64 }`). Discriminants на sum-
вариантах не были специфицированы.

**Что стало:** [D52](../02-types.md#d52) переписал систему целиком:

```nova
type UserId u64                          // newtype (Go-style, без =)
type StringMap[V] alias HashMap[str, V]  // alias через keyword
type Color | Red | Green | Blue          // sum через leading |
type ErrorCode | NotFound = 404 | InternalError = 500   // sum + discriminants
type User { id u64, name str }           // record без = (как было)
```

**Почему пересмотрели:**

- D17-правило «`=` для выражений типа» спотыкалось на sum-type:
  `type Color = Red, Green, Blue` — справа не «выражение типа», а
  список конструкторов. Натяжка.
- Newtype как first-class запрашивался для domain-modeling
  (`type Email str`, `type Score f64`) без шумной record-обёртки.
- Discriminants на sum-вариантах нужны для wire-протоколов
  (HTTP-коды, syscall-коды, serialization tags) — не были
  специфицированы.
- Парсер с D52 **однозначен по первому токену** после имени, нет
  напряжения «`=` иногда есть, иногда нет».
- `protocol` остаётся отдельным keyword'ом — D42 не пересматривается.

**Цена:** все существующие type-объявления переписать (`type X = Y` —
запрещено). Кода пока мало, миграция разовая.

**Связанные D:** [D17](../02-types.md#d17) (revised → D52),
[D52](../02-types.md#d52) (active), [D42](../02-types.md#d42)
(`protocol` без изменений).

### Объявление protocol revised: D42 → D53

**Что было:** `protocol` — отдельный keyword, рядом с `type`:

```nova
protocol Hashable {
    hash() -> u64
    eq(other Self) -> bool
}
```

`type` — для данных, `protocol` — для поведения. Два keyword'а в
системе типов.

**Что стало:** [D53](../02-types.md#d53) сделал `protocol`
**kind-токеном** в системе D52 (наряду с `alias`):

```nova
type Hashable protocol {
    hash() -> u64
    eq(other Self) -> bool
}
```

Все объявления типов идут через единый keyword `type`. Анонимный
protocol-тип в позиции параметра — `protocol { ... }` с обязательным
префиксом, симметрично `[]T`, `(A, B)`, `fn() -> T`. `any` = `type any
protocol { }` (top-type через пустой контракт) добавлен в prelude.

**Почему пересмотрели:**

- Асимметрия: `protocol Foo` объявлялся отдельным keyword'ом, но `Foo`
  использовался **в позиции типа параметра** (`fn f(x Foo)`). Программист
  спрашивал «если protocol — тип, почему не объявляется через type?».
- D52 ввёл `alias` как kind-токен — `protocol` встаёт в тот же ряд,
  усиливая системность.
- Прецедент Go (`type X struct { }`, `type X interface { }`) — единый
  keyword с kind-токеном.

**Цена:** все `protocol Foo { ... }` в spec/, decisions/, examples/
переписать в `type Foo protocol { ... }`. Кода мало, миграция разовая.

**Связанные D:**
- Старое: D42 (revised), D18 (revised — эффекты теперь через
  `type X protocol`).
- Новое: [D53](../02-types.md#d53).

### Операторы `as` и `is`: добавлены формально (D54)

**Что было:** `as` использовался без формального D-решения — упоминался
в [D44](../03-syntax.md#d44) (numeric literal coercion) и
[D52](../02-types.md#d52) (cast Sum→int, newtype↔underlying), но
не имел собственного блока. `is` не использовался — был свободным
keyword'ом.

После D53 в `any` появилась нужда извлекать конкретный тип
(`type-pattern-match` упоминался как открытый вопрос внутри D53).
Решено зафиксировать оператор отдельно.

**Что стало:** [D54](../03-syntax.md#d54) формализует пару:

- **`as`** — compile-time конвертация (numeric, newtype↔underlying,
  Sum→int). Возвращает целевой тип; невозможная конвертация — ошибка
  компиляции.
- **`is`** — runtime type-check **только для `any`-значений**.
  Возвращает `bool`. Pattern-форма `n is int` в `match` и `if` с
  smart cast (Kotlin-style) — переменная автоматически уточняется
  внутри ветки.
- Дополнительные методы на `any`: `try_as[T]() -> Option[T]` и
  `as[T]() Throws[TypeMismatch] -> T` для разных стилей extract'а.

**Почему пересмотрели:**

- D53 дал `any` через пустой protocol-тип, но не описал, как
  извлекать конкретный тип. Без `is`/`try_as[T]` `any` бесполезен в
  коде.
- Разделение `as`/`is` чётко: `as` — статически, `is` — runtime.
  Прецедент C#/Kotlin (`x is T`).
- `is` ограничен `any` — runtime-tag только для `any`-значений,
  локализованная стоимость. Расширять до sum-вариантов или
  protocol'ов — не нужно (есть `match`).

**Связанные D:** [D44](../03-syntax.md#d44) (численный `as`-cast как
частный случай), [D52](../02-types.md#d52) (newtype/sum-cast),
[D53](../02-types.md#d53) (`any`).

### Literal coercion: введено (D55)

**Что было:** sum-варианты требовали явный конструктор на каждом
значении (`Some(42)`, `Ok(user)`, `S("test")`); record-литералы
требовали имя типа перед `{}` (`User { id: 1, name: "alice" }`).
Это создавало визуальный шум, особенно для prelude-типов
(`Option[T]`, `Result[T, E]`) и в сигнатурах функций с record-аргументами.

**Что стало:** [D55](../02-types.md#d55) ввёл literal coercion в
позиции с явным целевым типом — два связанных правила:

- **Sum-coercion:** значение типа `S` оборачивается в единственный
  unary-конструктор `C(S)` sum-типа `T`. `let m Maybe[int] = 42` →
  `Just(42)`.
- **Record-coercion:** анонимный record-литерал `{ field: value }`
  получает имя из аннотации. `let u User = { id: 2, name: "Bob" }` →
  `User { id: 2, name: "Bob" }`.

Coercion **только** в позициях, где компилятор знает целевой тип
(аннотация `let`, аргумент функции, return-выражение, элемент
типизированной коллекции). В `let x = ...` без аннотации — литерал
сохраняет «свой» тип.

**Почему ввели:**

- Prelude-типы (`Option`, `Result`) — самые частые sum'ы, обёртки
  на каждом значении создают шум.
- Closed sum'ы (`SqlValue`, `JsonValue`) с coercion закрывают
  большую часть use-case'ов `any` — `Db.query(sql, args []SqlValue)`
  с `[42, "alice"]` теперь type-safe и эргономично.
- TS-style `const u: User = { id, name }` — известная эргономика для
  record'ов в позиции с типом. AI-friendly: имя типа из аннотации
  достаточно.

**Что отвергнуто (в рамках D55):**

- **Subtyping** (anonymous unions `string | number` без обёрток) —
  серьёзное расширение системы типов. Q-anonymous-union как
  возможный пересмотр.
- **Tuple-coercion** для multi-parameter конструкторов — отложено
  (двусмысленность с tuple-литералами).
- **Cross-type numeric coercion** (`42` → `f64` для `Number(f64)`) —
  Q-numeric-coercion, отложено до решения по `JsonValue`.
- **Record-coercion для sum-вариантов с record-формой** — программист
  обязан писать имя варианта (иначе type-driven parsing).

**Связанные D:** [D52](../02-types.md#d52) (sum), [D17/D52](../02-types.md#d52)
(record), [D44](../03-syntax.md#d44) (numeric literal coercion как
prior art), [D54](../03-syntax.md#d54) (`as`/`is` остаются явными).

### Embed alias: optional → mandatory (D39 revised)

**Что было:** D39 разрешал `use Type` без явного имени — поле получало
имя самого типа (Go-style), `use Account` → поле `Account`. Alias
`use name Type` использовался только при конфликтах или для
читаемости.

**Что стало:** [D39](../02-types.md#d39) revised — alias **обязателен
всегда**. `use Account` без имени → ошибка компиляции. Программист
пишет `use account Account`.

**Почему пересмотрели:**

- Default-имя по типу нарушало [D30](../03-syntax.md#d30): поля
  Nova — snake_case, типы — PascalCase. `use Account` → поле
  `Account` (PascalCase) — исключение в правиле naming.
- В одном record-блоке выглядело несогласованно: `audit_log` (snake)
  и `Account` (Pascal) рядом.
- Magic auto-conversion (`HashMap` → `hash_map`?) — не очевидное
  правило, AI-unfriendly.
- Прецедент Rust/Swift — все требуют явного имени поля.

**Цена:**

- Все `use Type` в spec/examples переписать на `use name Type`.
- В коде `examples/stdlib_set.nv` поправлено: `use HashMap[T, ()]` →
  `use map HashMap[T, ()]`, `@HashMap.method()` → `@map.method()`.
- D1 пример в `01-philosophy.md` обновлён (`use Account` →
  `use account Account`).

**Связанные D:** [D39](../02-types.md#d39) (revised),
[D30](../03-syntax.md#d30) (naming convention — теперь без исключений).

### Range, Iter, for-in: формализация (D58)

**Что было:** `0..n` упоминалось в spec'е только в for-loop
([D38](../03-syntax.md#d38)). Range как тип, как expression-литерал,
как итератор — нигде формально не описан. `for x in c` использовался
как «implicit iter» по факту в `oxsar_port.nv`/`stdlib_hashmap.nv`,
но без D-решения.

`Iter[T]`-protocol тоже использовался де-факто (анонимный protocol
`{ mut next() -> Option[T] }` в сигнатурах), без формальной фиксации.

**Что стало:** [D58](../03-syntax.md#d58) объединил три связанных
правила:

1. `a..b` и `a..=b` — литералы Range в **любой expression-позиции**,
   не только в for. Разворачиваются в `Range { start, end, inclusive
   }`.
2. `Iter[T] protocol { mut next() -> Option[T] }` — формальный
   protocol в prelude (D26).
3. `for x in c` — implicit iter: если `c` имеет `next() -> Option[T]`
   — используется напрямую; если есть `iter()` — компилятор вставляет
   вызов; иначе ошибка.

**Почему пересмотрели:**

- Range в Nova появлялся в for-loop как «магия». Без формализации
  нельзя было писать `let r = 0..n`, `fn count(r Range)`,
  `[]Range`-массивы.
- Anonymous protocol `{ mut next() -> Option[T] }` повторялся в
  сигнатурах — нужно именованное `Iter[T]`.
- `for x in c.iter()` — лишний `.iter()` каждый раз; прецеденты
  Kotlin/Swift/Python/Rust подтверждают implicit-сахар.

**Цена:**

- Prelude растёт (Range, RangeIter, Iter[T]).
- Парсер должен принимать `a..b` в любой expression-позиции (легко).
- `for-in` desugaring требует type-resolution для выбора между
  «прямое использование» и «.iter()».

**Связанные D:** [D58](../03-syntax.md#d58),
[D26](../08-runtime.md#d26) (prelude расширен),
[D38](../03-syntax.md#d38) (range в for — теперь частный случай D58).

### Vec[T] removed; methods on []T

**Что было:** `examples/stdlib_vec.nv` объявлял `type Vec[T] alias
[]T` и методы расширения. Vec был «именованной alias-обёрткой» над
`[]T`, без runtime-различия.

**Что стало:** Vec удалён совсем. `examples/stdlib_vec.nv` теперь
содержит только методы расширения **на `[]T` напрямую** (`fn []T
@map`, `@filter`, `@fold`, etc.). Vec нигде в spec/examples не
упоминается; везде `[]T` — единая каноническая форма
динамического массива.

**Почему пересмотрели:**

- Vec как alias не давал выгоды — `Vec[int]` ≡ `[]int`. Имя Vec
  только добавляло когнитивную нагрузку.
- Конструкторы Vec.new()/with_capacity() дублировали `[]` и
  `[]T.with_capacity(...)` (см. Q-array-api).
- `from_range` отложен в Q-collect-mechanism — без bound'ов на
  дженериках generic-collect не делается.
- Единая форма проще для AI и человека: «массив = `[]T`», ничего
  больше.

**Каскадные правки:**

- `examples/stdlib_vec.nv` переписан целиком — методы только на `[]T`.
- `examples/stdlib_queue.nv`: поля `Vec[T]` → `[]T`.
- `examples/stdlib_set.nv`, `examples/stdlib_linkedlist.nv` —
  упоминания Vec в комментариях заменены.
- `editors/vscode/`: `Vec` убран из prelude-types и подсветки.
- `spec/decisions/08-runtime.md`: `Vec` удалён из перечисления
  не-prelude коллекций.
- `spec/syntax.md`: пример generic'ов через `HashMap`/`[]T`.

**Связанные D:** [D52](../02-types.md#d52) (alias-форма — теперь не
для Vec), [D58](../03-syntax.md#d58) (Range — заменяет потенциальный
Vec.from_range).

### Field punning расширен и обязателен (D52)

**Что было:** D17 ввёл field punning только для **переменных в scope**:

```nova
let key = "alice"
let value = 42
let entry = Entry { key, value }                  // shorthand
let entry = Entry { key: key, value: value }      // тоже валидно (избыточно)
```

Обе формы равнозначны. **Два пути к одному результату** — anti-pattern
по AI-first ([D10](../01-philosophy.md#d10)).

Для `@field`-доступов (записи self-полей в record-литерал)
shorthand отсутствовал:

```nova
fn Range @iter() -> RangeIter =>
    { end: @end, inclusive: @inclusive, cur: @start }
//    ^^^^^^^^^^ ^^^^^^^^^^^^^^^^^^^^^^ повторяющийся @field
```

**Что стало:** [D52](../02-types.md#d52) расширяет field punning двумя
правилами:

1. **`{ @field }` — shorthand для self-доступов:**
   ```nova
   { @end, @inclusive, cur: @start }
   ```
   Имя поля = `end`/`inclusive`, значение = `@end`/`@inclusive`.

2. **Shorthand обязателен**, когда имя поля совпадает с источником:
   ```nova
   Entry { key: key }       // ✗ ОШИБКА — используйте { key }
   { end: @end }            // ✗ ОШИБКА — используйте { @end }
   { name: user_name }      // ✓ имя поля ≠ источника
   ```

**Почему пересмотрели:**

- Два пути к одному результату — AI-unfriendly. LLM генерирует
  случайно, code review не имеет правила.
- `@field`-shorthand отсутствовал — пропуск симметрии. `@field` —
  такой же first-class accessor (D35), как переменная в scope.
- Запрет избыточной формы — последовательность с D40/D43-стилевой
  философией Nova («один способ для одного случая»). Прецедент Rust
  имеет lint, но не язык; Nova идёт строже ради единообразия.

**Цена:**

- Все `Entry { key: key }`-формы в spec/examples переписаны на
  `{ key }`.
- Все `{ field: @field }` — на `{ @field }`.
- Несколько исторических примеров и конструкторов исправлено
  (`Account.new`, audit middleware, RangeIter constructor).

**Связанные D:** [D52](../02-types.md#d52) (расширение и запрет),
[D17](../02-types.md#d17) (исходное field punning), [D35](../03-syntax.md#d35)
(`@field`-доступ), [D40](../03-syntax.md#d40), [D43](../03-syntax.md#d43)
(прецеденты «один способ»).

### Array, tuple и позиционные partial patterns: формализация (D59)

**Что было:** D17/D52 фиксировали partial-pattern `..` только для
record-формы (`Occupied { value, .. }`). Array-patterns (`[]`,
`[r]`, `[_, ..]`), tuple-patterns (`(a, b)`), позиционные partial
(`Cons(..)`, `Move(x, ..)`) использовались де-факто в examples
(`effect-density/repository.nv`, `orm_demo.nv`,
`stdlib_linkedlist.nv`), но **формального D-блока** не существовало.

Q-positional-partial-pattern ставил вопрос только про позиционные
конструкторы sum.

**Что стало:** [D59](../03-syntax.md#d59) объединил **три родственных
паттерна** в один D-блок:

1. **Array patterns:** `[]`, `[x]`, `[a, b]`, `[head, ..]`, `[..,
   last]`, `[a, .., z]`, `[head, ..rest]` со slice-bind остатка.
2. **Tuple patterns:** `(a, b)`, `(a, _, c)`, destructuring let
   `let (a, b, c) = tuple`. Без `..` (длина известна типом).
3. **Positional sum partial:** `Cons(..)`, `Cons(h, ..)`, `Move(.., z)`
   — `..` как в массиве.

Единый смысл `..` во всех partial-формах: «остальные элементы
игнорируются».

**Почему пересмотрели:**

- Examples уже использовали без формализации. Парсер не знал
  грамматику, LLM не знала правила.
- Прецедент Rust — все три формы с одинаковым синтаксисом, проверено.
- Объединение трёх родственных правил в один D — паттерн
  D50/D58 (когда правила взаимно поддерживают друг друга).

**Цена:**

- Парсер расширяется на три формы. Стандартное.
- Exhaustiveness check для массивов сложнее (длина динамическая) —
  wildcard `_` обязателен в array-match без полного покрытия.
- Slice-bind `..rest` требует runtime-сегмента (zero-copy slice
  по [D32](../02-types.md#d32)).

**Связанные D:** [D59](../03-syntax.md#d59) (новое),
[D17](../02-types.md#d17), [D52](../02-types.md#d52) (record-partial
— основа), [D27](../03-syntax.md#d27) (`[]T`), [D34](../03-syntax.md#d34)
(`if let` с array/tuple-patterns).

Q-positional-partial-pattern закрыт.

### Spread `...x` в литералах: массив и record (D60)

**Что было:** Парсер D27 (`[]T`-литералы) и D17/D52 (record-литералы)
не знали о spread. Чтобы вставить элементы массива в массив, программист
писал `arr1.concat([4, 5]).concat(arr2)` — цепочка методов. Чтобы
обновить одно поле record'а, копировался каждый field вручную:
`{ id: u.id, name: "bob", email: u.email, age: u.age, ... }`.

**Что стало:** [D60](../03-syntax.md#d60) добавил **spread `...x`** в
литералах:

1. **Массив:** `[0, ...arr1, 4, ...arr2, 9]` — несколько spread
   разрешены. Тип каждого `...x` — `[]T`, совместимый с типом массива.
2. **Record:** `{ ...user, name: "bob" }` — base-record, затем
   override-поля. В MVP — **один spread на record-литерал**, всегда
   первый. Spread источник — record совместимого типа (структурно).

Совместимо с D52 literal coercion и field punning: `{ ...user, name }`
работает.

**Почему добавили:**

- AI-first: typical record-update в LLM генерациях — pattern
  `{ ...obj, field: v }`. Без него LLM пишет вручную, делает ошибки
  (пропускает поля).
- Backend boilerplate: «обновить один field» — частая операция, без
  spread даёт O(n) текста на каждое обновление.
- Прецедент: JS, TS, Python (`{**dict, k: v}`), Rust (struct update
  syntax `..base`), Swift, Kotlin (data class `copy`).

**Цена:**

- Парсер: новый non-terminal в array-/record-литералах. Стандартное.
- Type-checking spread: проверка совместимости — простая (то же что
  `concat`/`merge`).
- Runtime: array spread — копия элементов; record spread — копия полей.
  В hot path можно оптимизировать, но MVP без специальных трюков.

**Что отвергнуто:**

- `..arr` (две точки) — конфликт с partial-pattern (D59) и range-литералом
  (D58).
- `*arr` / `**obj` (Python) — `*` уже занят умножением; визуально шум.
- OCaml `with`: `{ user with name: "bob" }` — keyword `with` уже занят
  под effect-binding (D11), путаница неизбежна.
- Многократный spread в record-литерале — отложено в Q-record-spread-merge
  (нужно решить про конфликт ключей).
- Spread в pattern: `match xs { [1, ...rest, 5] => ... }` — D59 уже
  ввёл `..rest` для slice-bind в pattern; spread в pattern остаётся
  Q.

**Связанные D:** [D60](../03-syntax.md#d60) (новое), [D17](../02-types.md#d17)
(record-литералы), [D27](../03-syntax.md#d27) (array-литералы),
[D52](../02-types.md#d52) (literal coercion + field punning),
[D58](../03-syntax.md#d58) (range — другой смысл `..`),
[D59](../03-syntax.md#d59) (partial pattern — другой смысл `..`).

### Тело функции, лямбды и handler-method: единый закон «=> и {} не сочетаются»

**Что было:** [D22](../03-syntax.md#d22) и [D40](../03-syntax.md#d40)
вместе допускали и `fn name(...) => expr`, и `fn name(...) { block }`,
и лямбду `(params) => { block }` (через сочетание правил).
[D23](../03-syntax.md#d23) разрешал guard-цепочки в `=>`-теле через
`return`. [D31](../04-effects.md#d31) показывал handler-method
`exec(p) => { stmts; resume(()) }`. [D43](../03-syntax.md#d43)
называл `f(args) { params => body }` «trailing-lambda».

Это был не один закон, а набор пересекающихся правил: лямбда могла
иметь блок-форму через `=> { ... }`, handler-method тоже, fn — тоже.
Граница «выражение vs блок» размывалась.

**Что стало:** ревизия D22+D40+D43+D31+D23+D19 фиксирует **общий
закон**: `=>` и `{}` не сочетаются — ни для `fn`, ни для лямбд, ни
для handler-method. Match-arm — **единственное исключение** ради
гарантированного маркера «начало результата» после pattern'а с возможным
guard'ом.

| Контекст       | `=> expr` | `{ block }` | `=> { block }` |
|----------------|-----------|-------------|----------------|
| `fn name(...)` | ✅         | ✅           | ❌              |
| Лямбда         | ✅         | ❌           | ❌              |
| Match-arm      | ✅         | —           | ✅ (исключение) |
| Handler-method | ✅         | ✅ (без `=>`) | ❌            |

[D43](../03-syntax.md#d43) переименован: «trailing-lambda» →
**trailing-block**. Это не лямбда (лямбда строго `=> expr`), а
самостоятельная грамматика `f(args) { [params =>] stmts; expr }`.
Синтаксис не изменился — изменилась только классификация.

[D23](../03-syntax.md#d23) уточнён: guard-цепочки `if cond { return }`
требуют **блок-формы** `fn name(...) { ... }`. Раньше D23 показывал
`fn classify(x) -> str => if x < 0 { return "n" } ... "big"` — это
противоречило D40 («`=>` = одно выражение»).

**Почему пересмотрели:**

- **AI-first.** Пять контекстов (fn-body / lambda-body / match-arm /
  handler-method / trailing-block) с пересекающимися правилами —
  невозможно надёжно держать в голове, ни LLM, ни человеку.
- **Один закон + одно исключение** — компактнее и проверяемее, чем
  «всё иногда сочетается».
- **Лямбда как значение-выражение.** В Nova лямбда — first-class
  значение в выражении. Если нужен блок с `let`'ами и statement'ами —
  это уже named fn, а не лямбда.
- **Trailing-block ≠ лямбда.** Хотя синтаксически `f() { stmts }`
  раньше называлось trailing-lambda, семантически это блок-аргумент
  к вызову, не значение-функция. Переименование делает разницу видимой.

**Цена:**

- Большой sweep: ~10 файлов в `examples/` переписаны (audit, oxsar_port,
  effect-density/{repository,service,http,main}, orm_demo, и др.).
  Лямбды с блоками вынесены в named fn'ы (audit_step, recover_http,
  log_audit_failure). `fn ... => { block }`-формы переведены на
  блок-форму `fn ... { block }`. Handler-method'ы с `=> { block }`
  переведены на `op(p) { block }`.
- Несколько примеров в самой спеке (D40, D23, D31, revolutionary.md,
  06-concurrency.md, 02-types.md, 08-runtime.md, syntax.md, open-questions.md)
  обновлены под новый закон.
- Match-arm остаётся с двумя формами `pattern => expr` и
  `pattern => { block }` — ради `=>` как маркера. Это компромисс,
  обоснованный в D19.

**Связанные D:** [D22](../03-syntax.md#d22), [D40](../03-syntax.md#d40),
[D43](../03-syntax.md#d43), [D23](../03-syntax.md#d23),
[D19](../03-syntax.md#d19), [04-effects.md → D31](../04-effects.md#d31).

### Полная семантика эффектов (D61)

**Что было:** Спека эффектов до D61 имела зияющую дыру. Ключевые
вопросы оставались без ответа:
- Что формально означает `resume(v)`?
- One-shot или multi-shot? Что при повторном вызове?
- Тип `Handler[E]`, как объявлен, какие операции?
- Запрет `resume` для Never-операций?
- Тип результата `with`-блока?
- Алгоритм компиляции/интерпретации эффектов?

`resume` использовался во всех handler-литералах в spec и examples,
`Handler[Db]` фигурировал в декораторах ([orm_decorators.nv](../../examples/orm_decorators.nv)),
но D-блока не было — каждый имплементатор должен был догадываться.
Ровно эту дыру нашёл агент при ревью спеки.

**Что стало:** [D61](../04-effects.md#d61) закрыл всё в одном большом
блоке. Ключевые решения:

1. **`type Db effect { ops }`** — отдельный keyword `effect` для
   объявления типа эффекта (вместо ранее использовавшегося `protocol`).
   Эффект и protocol — семантически разные контракты (статический
   dispatch vs lookup в with-стеке), их смешение в одном keyword'е
   создавало путаницу. Раздельные keyword'ы запрещают смешение
   compile-time.

2. **`handler Db { ops }`** — keyword для handler-литерала. Раньше
   `Db { query(q) => ... }` различался от record-литерала только
   эвристикой парсера. Явный `handler` keyword однозначен.

3. **`Handler[E]`** — first-class тип значения handler-литерала.
   Появляется в let-биндингах, return-position функций, аргументах
   (handler-декораторы). Стандарт литературы (Eff, Koka, Effekt).

4. **`return v` / финальное выражение для нормального завершения** —
   handler-method ведёт себя как обычная функция. Возвращаемое
   значение идёт в caller операции (continuation возобновляется).
   Никакого `resume` keyword'а — у пользователя без опыта алгебраических
   эффектов «handler возвращает значение» точнее передаётся через
   обычный return, чем через резко новое слово.

5. **`interrupt v`** — единственный новый keyword. Досрочное завершение
   всего `with`-блока, значение `v` становится результатом with.
   Используется для Throws-handler'ов (handler решает что вернуть
   при throw без выполнения continuation) и для редких случаев
   досрочного прерывания обычной операции.

6. **One-shot**, tail-position для `return`/`interrupt`. Полная
   continuation-семантика (multi-step, multi-shot) отложена под
   Q-multishot-resume — backend Nova не нуждается.

7. **Effect-row неупорядочен, дубликаты запрещены.** `Db Logger` и
   `Logger Db` — одна сигнатура. `Db Db` — compile error.

8. **Прямой `h.op(args)`** на handler-значении, минуя with-стек —
   нужен для handler-декораторов.

9. **Тип `with`-блока** — единый тип `T`: финальное выражение body
   и все handler-method'ы (когда они не делают `interrupt`) обязаны
   возвращать `T`.

10. **Раздел «Алгоритм компиляции/интерпретации эффектов»** —
    пошаговое тех-задание для имплементатора. Что делает компилятор
    для каждой конструкции, что делает runtime, какие проверки.
    Без этого раздела имплементации расходились бы.

**Почему пересмотрели:**

- **Дыра в спеке** была обнаружена пользователем при ревью.
  `resume`/`Handler[E]` использовались, но не определены — нельзя
  написать совместимый компилятор.
- **AI-first**. `resume` как keyword требует объяснения концепции
  «continuation, которая возобновляется». Это сложно для пользователя
  без опыта Koka/OCaml. `return` + `interrupt` сводит handler к
  «обычная функция плюс escape» — на 95% случаев интуиция «return»
  работает 1:1.
- **Раздельные `effect` / `protocol`** — урок практики. После D53
  объединение породило путаницу: код использовал `protocol` для
  Db, но семантика отличалась.

**Цена:**

- ~30+ файлов в spec и examples с `type X protocol { ... }` для эффектов
  → переписать на `effect`. Handler-литералы (`Db { query(q) => ... }`)
  → `handler Db { ... }`. Throws-handler'ы — добавить `interrupt`
  явно.
- Bootstrap-компилятор требует доработки: парсинг `effect`/`handler`/
  `interrupt` keyword'ов, тип `Handler[E]`, прямой `h.op(args)`.
  Sweep большой, но детерминированный.
- Q-resume-semantics и Q-handler-method-param-inference закрыты через
  D61 (выбраны (II) tail-only и (A) inference из protocol-сигнатуры
  соответственно).

**Связанные D:** [D61](../04-effects.md#d61) (новое — закрывающее),
[D2](../04-effects.md#d2), [D11](../04-effects.md#d11),
[D18](../04-effects.md#d18) (revised — `protocol` → `effect`),
[D25](../04-effects.md#d25), [D31](../04-effects.md#d31),
[D53](../02-types.md#d53) (revised — расщепление `protocol`/`effect`).

### Прагматичная семантика эффектов: D62 — прямые в сигнатуре, Fail strict, Async ambient

**Что было:** D28 требовал чтобы public-функции декларировали
**все** эффекты в сигнатуре (включая через вложенные вызовы).
`Async` входил в стандартный набор эффектов и писался везде в
backend-сигнатурах. `Mut` упоминался в R2 как generic эффект.
Правило выбора `effect`/`protocol` было размытым.

В реальном backend-коде ([effect-density/](../../examples/effect-density/))
сигнатуры накапливали 8-10 эффектов на функцию, что неприемлемо
для AI-first-чтения и человеческого восприятия. Громоздкость
сигнатур стала блокером.

**Что стало:** [D62](../04-effects.md#d62) — финальная ревизия
философии эффектов:

1. **Прямые эффекты в сигнатуре**, не транзитивные. Функция объявляет
   только эффекты, чьи operations использует **сама**, не через
   вложенные вызовы. Транзитивные — warning'ом подсвечиваются.

2. **`Fail` strict** — исключение из правила «прямые». `Fail[E]`
   обязателен в сигнатуре везде, где может произойти throw, включая
   через границы вызовов. Это сохраняет проверку control-flow
   ошибок (как Java checked exceptions / Rust Result).

3. **`Async` — ambient capability**. Не пишется в сигнатурах, не
   является частью type system'ы. Fiber-runtime под капотом. R7
   переписана из «Async — эффект, не вирус» в «Async — невидимая
   инфраструктура».

4. **`Mut[T]` убран** из стандартного набора. Реальные сценарии
   покрываются специализированными эффектами (Counter, Cache, IdGen,
   etc.) с понятными именами или локальными `let mut x` без
   эффекта.

5. **Правило `effect` vs `protocol`** — два sniff-вопроса
   (with-substitution + continuation-capture). Сознательный выбор
   программиста; compile-time enforcement = последствие.

**Почему пересмотрели:**

- **Громоздкость реальных сигнатур.** Полная транзитивность давала
  максимально честные сигнатуры, но в backend-коде накапливала
  8-10 эффектов на функцию. Невозможно читать.
- **`Async` везде** — в реальном backend почти каждая функция
  «может приостановиться». Если он эффект — он шум без информативности.
- **`Mut[T]` — анти-паттерн.** Каждый раз, когда возникал — было
  лучше дать имя через специализированный эффект. Generic Mut[T]
  провоцировал безымянное shared state.
- **Правило `effect`/`protocol`** размывалось формулировками типа
  «есть объект или нет», что слабо для практической дисциплины.

**Цена:**

- **R5.2 ослаблена** — «сигнатура показывает прямые эффекты + полная
  throw-картина», не «полное описание поведения».
- **R5.6 ослаблена** аналогично — карта эффектов покрывает прямые
  использования, транзитивные через IDE/линтер.
- **R6 capability** — compile-time-гарантия только на closure-границах
  и через project-whitelist; не на всех границах вызовов.
- **R7 переписана** — Async-как-эффект убран, теперь невидимая
  инфраструктура.
- **Sweep ~30+ файлов** — убрать `Async` из сигнатур, обновить R2
  таблицу, переписать R-главы.
- **Bootstrap-компилятор**: warning для транзитивных эффектов,
  strict для Fail, опциональный атрибут `@allow_transit`.

**Связанные D:** [D62](../04-effects.md#d62) (новое — закрывающее
философию), [D28](../04-effects.md#d28) (revised — только прямые),
[D25](../04-effects.md#d25), [D61](../04-effects.md#d61).
**Связанные R-главы:** R5.2, R5.6, R6, R7 — все revised в
[revolutionary.md](../../revolutionary.md).

### Полная семантика `Fail`: D65 — гибрид Fail[E]/Fail, lookup, prelude RuntimeError/Error

**Что было:** D25 фиксировал `Fail[E]` для типизированных ошибок и
`Fail` без параметра как сахар над `Fail[Error]`, где `Error` это
unit-тип-маркер в prelude. Это работало, но имело пробелы:

- `Error` без полей был бесполезен — нечего было нести в throw.
- Семантика `Fail` без параметра была неясна — «универсальный сахар»,
  но без точного определения через какой тип.
- Lookup-правило handler'ов при `throw expr` нигде явно не описано.
- Поведение re-throw внутри handler'а не зафиксировано.
- Не было типа для встроенных runtime-ошибок (DivByZero, Overflow,
  IndexOutOfBounds) — они существовали как concept, но без D-блока.

**Что стало:** [D65](../04-effects.md#d65) объединяет всё в один
закрывающий блок:

1. **Гибрид `Fail[E]` / `Fail`**: типизированный для production,
   universal (= `Fail[any]`) для catch-all и quick-and-dirty.
2. **Lookup при throw**: точный тип `E` → catch-all `Fail` (any) →
   runtime panic.
3. **Match по sum-вариантам — внутри handler'а**, не через subtype-aware
   lookup. Один handler `Fail[RuntimeError]`, разбор внутри через
   match.
4. **Re-throw** через `throw err` в handler'е ищет outer handler.
5. **Prelude-типы**:
   - `RuntimeError` sum для встроенных runtime-сбоев (DivByZero,
     Overflow, IndexOutOfBounds, TypeMismatch, AssertFailed,
     NoHandler).
   - `Error` теперь record `{ msg str }` с фабрикой `Error.new(msg)`.

**Почему пересмотрели:**

- **Дискуссия выявила пробел**: `Fail[?]` syntax вопрос — что в нём
  должно стоять для quick-and-dirty? Что для встроенных runtime'ов?
  Что для пользовательских?
- **`Error` как unit-маркер был бессмыслен** — нечего бросать.
  Replacement на record с msg даёт понятную семантику.
- **`RuntimeError` нужен** — встроенные `a/b`/`arr[i]`/etc. должны
  иметь конкретный тип ошибки. Sum-тип в prelude покрывает.
- **Lookup-правило** требовалось формализовать — без него
  имплементаторы выбрали бы разные стратегии (subtype-aware vs
  exact-match), и compatibility ломалась бы.
- **Гибрид Fail[E] + Fail** — компромисс. Один путь (только typed)
  — неудобен для скриптов и тестов. Один путь (только universal) —
  теряется compile-time exhaustiveness. Гибрид с convention для
  public API — баланс.

**Цена:**

- Sweep по spec и examples — `transaction[T](body fn() Db Fail -> T)`
  переписать с явным generic-параметром `[E]` или `Fail` (any).
  Конкретные функции типа `parse(s) Fail` — типизировать.
- В bootstrap-prelude добавить `RuntimeError` sum и `Error` record.
- Type checker нужно расширить: «Fail (any) поглощает Fail[E]»;
  multi-Fail в row; lookup-правило при throw.
- Q-fail-coercion открыт — auto-coercion `E → E'` через однозначный
  sum-variant отложено.

**Связанные D:** [D65](../04-effects.md#d65) (новое — закрывающее
тему ошибок), [D25](../04-effects.md#d25) (уточняется — `throw` и
`Fail[E]`), [D26](../08-runtime.md#d26) (prelude обновлён —
`Error` стал record, добавлен `RuntimeError`),
[D62](../04-effects.md#d62) (Fail strict — уточняется совместимостью
типов).

### Capability sandbox и realtime: D63 + D64

**Что было:** `forbid` упоминался в R6 (revolutionary.md) как
keyword для capability-sandbox, но без формальной спеки. Не было
описано: compile-time + runtime механика, что разрешено внутри,
что значит «forbid Async». Аналогично, после удаления Async из
type system ([D62](../04-effects.md#d62)) не было способа гарантировать
«функция не приостанавливается» — это нужно для real-time-зон,
hot loops, lock-критичного кода.

**Что стало:** [D63](../04-effects.md#d63) и [D64](../04-effects.md#d64)
формализуют две связанные runtime-конструкции:

- **`forbid X1, X2 { body }`** — sandbox для type-system эффектов.
  Compile-time error при прямых нарушениях, runtime барьер через
  sentinel-frame в handler-стеке для транзитивных. Установка нового
  handler для forbid-эффекта внутри — compile error (sandbox
  непреодолим). `forbid Async` явно запрещён — Async не в типах.

- **`realtime { body }`** — runtime-зона, гарантирующая что код не
  приостанавливается на yield-point'ах. Не эффект, а runtime-флаг
  fiber-runtime'а. Запрещает suspend-операции (Net, Fs, Db, Time.sleep,
  Channel.recv, spawn). Опциональный модификатор `realtime nogc` —
  запрет аллокации в managed heap. Атрибут `@realtime` на функции —
  sugar для функции целиком.

**Почему два механизма, а не один:**

- `forbid` работает с **type-system эффектами** — там есть имя в
  типе, можно проверить compile-time.
- `realtime` работает с **невидимой инфраструктурой** (fiber-suspend,
  GC pause) — нет имени в типе, только runtime-флаг.

Async-концепт **полностью удалён из языка**. Программист про него не
знает; есть только `realtime` как inverse-маркер «гарантированно
sync-зона».

**Цена:**

- Bootstrap-компилятор: lexer keyword'ы `forbid`, `realtime` (опционально),
  AST/parser/interp — добавить.
- Type checker: compile-time проверка для forbid (прямые эффекты);
  частичная для realtime (известные suspend-операции).
- R6 в revolutionary.md ссылается на D63 как формализацию.
- R7 уже обновлена под D62 («Async — невидимая инфраструктура»);
  D64 завершает картину inverse-маркером `realtime`.

**Связанные D:** [D63](../04-effects.md#d63) (новое), [D64](../04-effects.md#d64) (новое),
[D62](../04-effects.md#d62) (Async ambient — основа для D64),
[D11](../04-effects.md#d11) (with — параллель с forbid),
[D14](../06-concurrency.md#d14) (fiber runtime — где realtime ставит флаг).

### `Self` universal: D66 убирает «только в protocol»

В D42 `Self` был ограничен только protocol-объявлениями. Это
ограничение унаследовано от первой редакции, где Self вводился именно
для type-safe equality (`eq(other Self) -> bool`). На практике
оказалось что Self полезен и в:

- static-методах (`fn Box[T].of(v T) -> Self`) — DRY вместо повтора
  `Box[T]`,
- instance-методах (builder pattern: `fn User @with_name(n str) -> Self`),
- effect-методах (transactional `nested(body fn() Self -> ())`),
- sum-варианте (`fn Tree @clone() -> Self`).

D66 убрал ограничение: `Self` валиден в любом type-контексте. Семантика
одна — «текущий тип, к которому принадлежит метод/контракт». Аналогично
Swift/Rust.

**Связанные D:** [D66](../02-types.md#d66) (новое),
[D42 (REVISED)](../02-types.md#d42) (исходный Self в protocol),
[D53](../02-types.md#d53) (унификация type/protocol).

### `?` оператор для Option: D67 фиксирует семантику

В D4 `?` был определён только для `Result[T, E]` через эффект `Fail[E]`.
Для `Option[T]` оператор работал де-факто в bootstrap-интерпретаторе
через ранний `return None`, но это не было зафиксировано —
[08-runtime.md → D26](../08-runtime.md#d26) явно перечислял этот
вопрос как открытый.

D67 формализует обе семантики:
- `?` на `Result[T, E]` → `match Ok(v) => v, Err(e) => throw e`
  (через эффект Fail, как в D4).
- `?` на `Option[T]` → `match Some(v) => v, None => return None`
  (ранний return из функции, без эффекта).

Также D67 явно **запрещает** `?` после вызова, который бросает через
эффект Fail напрямую (`real.in_transaction(b)?` где
`in_transaction Fail -> T`) — это синтаксическая ошибка. Throw сам
пробрасывается через Fail-эффект caller'а, без `?`. Эта частая
ошибка при написании middleware-handler'ов теперь явно отмечена.

**Связанные D:** [D67](../04-effects.md#d67) (новое),
[D4](../04-effects.md#d4) (исходный `?` для Result),
[D62](../04-effects.md#d62) (Fail strict транзитивность).

### Stateful handlers: D68 формализует два паттерна

В D11/D61 handler — это значение, содержащее **только методы**
операций. Поля внутрь handler-литерала добавлять нельзя.

Stateful handlers (handler'ы со своим состоянием) делались де-факто
через closure-capture (state в `let mut x` снаружи `with`,
handler-методы захватывают `x`). Это работало во всех `tests-nova/`
и `examples/*.nv`, но как «канонический паттерн» не было зафиксировано.

D68 формализует **два** паттерна:
- **Closure capture** — лёгкий, для тестов и одноразовых handler'ов.
- **Record + `@as_handler` метод** — для случая когда state нужно
  проинспектировать после `with`-блока (типичный testing-сценарий).

Также D68 явно описывает семантику `@field` внутри handler-литерала
созданного в `@`-методе record'а: `@` ссылается на receiver внешнего
метода (handler полей не имеет).

**Связанные D:** [D68](../04-effects.md#d68) (новое),
[D11](../04-effects.md#d11), [D31](../04-effects.md#d31) (handler-лямбда),
[D35](../03-syntax.md#d35) (`@`-методы), [D61](../04-effects.md#d61).

### Variadic-параметры: D69 формализует `print(...)` use-case

В bootstrap-stdlib `print`/`println` изначально были Native-функциями,
принимающими переменное число аргументов (через Rust-side `&[Value]`).
Но в спеке D26 объявлял `fn print(s str) Io -> ()` — фиксированную
arity 1. Это drift между bootstrap и spec.

D69 формализует variadic как полноценную фичу языка через TypeScript-
style синтаксис: `fn print[T](...items []T) Io -> ()`.

Решающие выборы:
- **Prefix `...`** (как D60 spread в литералах) — symmetric, не Go-style postfix.
- **Тип параметра `[]T`** (как TS) — не element type как в Go. «Один
  тип, две формы вызова».
- **Только последний параметр** может быть variadic (упрощение).
- **Mix explicit + spread** разрешён: `f("x", ...arr, "y")`.
- **Heterogeneous через `any`**: `print(...items []any)` использует
  D54 top-type. Каждый элемент через `to_str()`.

`print`/`println` в D26 переписаны на variadic-сигнатуру.

**Связанные D:** [D69](../03-syntax.md#d69) (новое),
[D60](../03-syntax.md#d60) (spread в литералах — symmetric),
[D54](../03-syntax.md#d54) (any), [D26](../08-runtime.md#d26)
(prelude print/println).

### `ToStr` protocol: D70 формализует to_str() как первоклассную фичу

В bootstrap-stdlib `to_str(v)` работал как Native-функция на любом
значении (через Rust `format!("{}", v)`), но в спеке формального
определения protocol'а не было.

D70 формализует:
- `ToStr` protocol в prelude с методом `@to_str() -> str`.
- Auto-derive для всех встроенных типов и record/sum-комбинаций.
- Override через обычный `@to_str()` метод на пользовательских типах.
- Free function `to_str[T: ToStr](v T) -> str` — публичный API.
- String interpolation `"${expr}"` — sugar над `to_str(expr)`.
- D69 variadic `print(...items []any)` использует `to_str` для
  каждого элемента.

Имя `ToStr` выбрано буквальным (не `Display` как Rust, не `Show` как
Haskell, не `Stringer` как Go) — описывает что метод делает, без
конфликта с UI-кодом (`Slide.show()`, `popup.display()`).

Альтернатива через универсальный `@cast[X]` метод **отвергнута**:
- `[X]` грамматически объявляет generic-параметр (D16), не target.
- Return-type dispatch потребовал бы typeclass-механизм.
- Конкретные конверсии через отдельные protocol'ы (`ToStr`, `ToJson`,
  `ToBytes`) — D46 overloading по имени работает естественно.

**Связанные D:** [D70](../08-runtime.md#d70) (новое),
[D26](../08-runtime.md#d26) (prelude), [D35](../03-syntax.md#d35)
(@-методы), [D69](../03-syntax.md#d69) (variadic print через to_str),
[D46](../03-syntax.md#d46) (overloading методов).

## Как читать историю

- **«revised»** в статусе D — текст переписан, решение действует, но
  отличается от первоначальной формулировки.
- **«cancelled»** — решение отменено и заменено другим.
- **«active»** — решение в текущей форме без пересмотров.

Все «cancelled» решения помечены в начале блока `> ⚠️ ОТМЕНЕНО, см. DZZ`.
