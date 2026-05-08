# Runtime — режимы запуска, panic, prelude, статическое состояние

Решения этой группы определяют, как программа Nova **исполняется**:
поддерживаемые режимы компиляции, что считается panic'ом и как он
обрабатывается, что предоставляет prelude и почему в языке нет
static-состояния.

| # | Решение |
|---|---|
| [D7](#d7-один-язык--три-режима-компиляции) | Один язык — три режима компиляции |
| [D13](#d13-panic-vs-эффекты-что-не-является-эффектом) | Panic vs эффекты: что НЕ является эффектом |
| [D26](#d26-базовая-stdlib-и-prelude) | Базовая stdlib и prelude |
| [D41](#d41-static-функции-есть-static-состояния-нет) | Static-функции есть, static-состояния нет |
| [D70](#d70-tostr-protocol--to_str-метод--free-function-tostrv) | ⚠️ REPLACED → D73. `ToStr` protocol + `@to_str()` (историческая справка) |
| [D73](#d73-from--into-protocol-пара-с-авто-выводом) | `From` / `Into` protocol-пара с авто-выводом |
| [D74](#d74-математические-операции-на-числовых-типах--instance-методы) | Математические операции на числовых типах — instance-методы |
| [D77](#d77-tryfrom--tryinto-protocol-пара-расширение-d73-для-fallible-конверсий) | `TryFrom` / `TryInto` — расширение D73 для fallible-конверсий |
| [D76](#d76-mem-эффект--runtime-introspection-для-leakgrowth-тестов) | `Mem` эффект — runtime introspection для leak/growth тестов |
| [D81](#d81-assertcond-vs-debug_assertcond--build-mode-семантика) | `assert(cond)` vs `debug_assert(cond)` — build-mode семантика |

---

## D7. Один язык — три режима компиляции

### Что
Один и тот же исходник Nova поддерживает три режима исполнения:
**AOT** (бинарь, как Go), **JIT** (как .NET) и **интерпретатор**
(как Python). Скрипт за 1 строку и сервер на 100k строк — это
разные режимы запуска одного языка, а не разные языки.

### Правило

```bash
nova run script.nv          # интерпретатор / JIT (быстрый старт)
nova build app.nv           # AOT-бинарь, как `go build`
nova jit-server             # долгоиграющий процесс с JIT-компиляцией
```

Один и тот же `script.nv` без модификации работает во всех трёх
режимах. Эффекты, типы, контракты, handler'ы — везде ведут себя
одинаково.

### Почему

- **Скрипт vs сервер — это режимы запуска.** Не разные языки.
  Программисту не нужно «переписывать» под другой режим.
- **Прецедент Julia** — тот же подход (JIT по умолчанию + AOT через
  `PackageCompiler.jl`) работает на масштабе data-science.
- **AI-first** — LLM может генерировать код и запускать через
  интерпретатор для быстрой проверки, а тот же код собирать в бинарь
  для production.
- **Эффекты ортогональны runtime'у** — handler'ы перехватываются и в
  JIT, и в AOT, и в интерпретаторе одинаково.

### Что отвергнуто

- **Только AOT** (Rust/Go-стиль) — медленный feedback loop, плохо
  для скриптов и REPL.
- **Только интерпретатор** (Python) — производительность недостаточна
  для backend.
- **Транспиляция в чужой язык** (TypeScript → JS) — теряется
  возможность контроля runtime, привязка к чужой экосистеме.

### Связь

- [01-philosophy.md → D9](01-philosophy.md#d9-честная-оценка-новизны) —
  «три режима компиляции в строго типизированном языке» — одна из двух
  потенциальных уникальных заявок Nova.
- [01-philosophy.md → D10](01-philosophy.md#d10) — три режима следуют
  из «всё — эффект»: handler'ы абстрагируют runtime.

### Открытые вопросы

- Конкретные технологии: LLVM для AOT? Cranelift для JIT? Tree-walking
  для интерпретатора? — выбор реализации.
- Совместимость артефактов между режимами — пока считаем, что один
  исходник, разные бинарные форматы.

---

## D13. Panic vs эффекты: что НЕ является эффектом

### Что
**Не каждое прерывание вычисления — эффект.** Аппаратные/математические
сбои (деление на ноль, выход за границы массива, переполнение, OOM,
переполнение стека) **не указываются в сигнатуре** функции. Они
образуют общую категорию `Panic` — runtime-сбоев, перехватываемых
runtime'ом на границе fiber'а, не программистом в коде.

### Правило

#### Граница

| | Видимое (в сигнатуре) | Универсальное (не в сигнатуре) |
|---|---|---|
| **Что** | эффекты, описывающие **намерение** | сбои, описывающие **невозможность вычисления** |
| **Примеры** | `Net`, `Db`, `Time`, `Log`, `Fail[BusinessError]` | деление на ноль, переполнение, выход за границы, OOM, переполнение стека |
| **Где ловится** | handler'ом в коде | runtime'ом на границе fiber'а |
| **Как создаётся** | `throw` | `panic(msg)` или сам runtime |

#### Перехват — на границе fiber'а runtime'ом

`panic` концептуально означает **смерть текущего fiber'а**, не
процесса. В синхронной программе без fiber'ов (CLI, скрипт) fiber один
= процесс, поэтому panic = exit. В серверной программе с fiber-runtime
([06-concurrency.md → D14](06-concurrency.md#d14)):

- **HTTP-handler** — fiber на запрос. Panic = смерть fiber'а, runtime
  возвращает 500, остальные запросы продолжают.
- **Worker очереди** — fiber. Panic = задача упала, scheduler берёт
  следующую.
- **Supervised group** — supervisor видит «fiber завершился panic'ом»,
  рестартует по своей стратегии.

```nova
fn handle_request(r Request) Db Log -> Response =>
    process(r)             // если panic — fiber умирает, runtime вернёт 500
                            // если throw — handler выше ловит обычно

fn server() Net Fail -> () {
    supervised {
        spawn handle_requests()
        spawn periodic_cleanup()
    } strategy = one_for_one, max_restarts = 3
    // supervisor рестартует упавшие fiber'ы
}
```

**Никакого `try_panic`/`catch` в коде.** Программист **не ловит**
panic в обычной функции — это работа runtime'а на границе fiber'а.
Если программист хочет управляемую ошибку — пишет `throw` +
`Fail[E]`, ловит обычным handler'ом.

#### Унификация двух уровней ошибок

- **`throw` + `Fail[E]`** — управляемая ошибка, видна в сигнатуре,
  перехватывается handler'ом в коде ([04-effects.md → D25](04-effects.md#d25)).
- **`panic`** — сбой fiber'а, перехват только runtime'ом на границе
  fiber'а. В сигнатуре не виден.

Третьего уровня нет. Никаких `try_panic { ... } catch p { ... }` или
`panic_boundary { ... } recover (p) => { ... }` в языке.

#### Опция: строгий режим `@strict_total`

Для критичного кода (медицина, финансы, авионика):

```nova
@strict_total
fn critical(...) -> Result =>
    // деление на ноль здесь — compile error
    // обязаны checked-операции: safe_div(a, b)?, arr.get(i)?
```

Превращает функцию в тотальную (всегда завершается). Цена — больше
кода, но для 1% случаев это окупается.

### Почему

Если бы `Fail[DivByZero]` был обязателен, он бы появился в **каждой
второй сигнатуре** (любая функция со средним арифметическим,
дисперсией, делением). К нему присоединились бы `Fail[IntegerOverflow]`,
`Fail[ArrayBounds]`. Это **синдром Java checked exceptions** —
информативность сигнатуры исчезает, потому что эффекты везде.

Сознательный компромисс: **строгая теория эффектов уступает
читабельности** в зоне аппаратных сбоев.

#### Что НЕ Panic, а обычный эффект

- Бизнес-ошибки парсинга, валидации, аутентификации → `Fail[E]`.
- Network failure, DB connection refused → `Fail[NetError]`,
  `Fail[DbError]` внутри эффекта `Net` / `Db`.
- Любая ошибка, которую программа **намерена обрабатывать**, —
  это не Panic.

**Принцип:** «обработать никак нельзя, надо умереть» → Panic;
«обработать можно и нужно» → Fail.

### Что отвергнуто

- **`Fail[DivByZero]` для каждой функции** — спам в сигнатурах.
- **`try_panic`/`catch` в обычном коде** — путает с `Fail`,
  усложняет reasoning о потоке управления.
- **Panic как обычное Throwable** (Java RuntimeException) — приводит
  к ловле «всего» через `catch (Exception e)`, антипаттерн.

### Связь

- [04-effects.md → D25](04-effects.md#d25) — `throw` и `Fail[E]`.
- [06-concurrency.md → D14](06-concurrency.md#d14) — supervisor, fiber'ы.
- [01-philosophy.md → D10](01-philosophy.md#d10) — «всё — эффект» с
  оговоркой про runtime panics.

---

## D26. Базовая stdlib и prelude

### Что
Базовые типы (`Option[T]`, `Result[T, E]`, `Error`, `Never`,
`Ordering`) и их конструкторы (`Some`, `None`, `Ok`, `Err`) живут в
**prelude** — автоматически в скоупе любого модуля, без `import`.
Список prelude **явно зафиксирован** в одном месте, не «магия».

### Правило

#### Что в prelude (v1.0)

**Типы:**

```nova
type Option[T] | Some(T) | None
type Result[T, E] | Ok(T) | Err(E)
type Ordering | Less | Equal | Greater
type Never                                       // unit без значений (uninhabited)
type any protocol { }                            // top-type через пустой protocol (D53)
```

**Базовые методы `Option[T]`:**

```nova
fn Option[T] @is_some() -> bool
fn Option[T] @is_none() -> bool
fn Option[T] @unwrap() Fail[Error] -> T              // throw "called unwrap on None"
fn Option[T] @unwrap_or(default T) -> T              // None → default
fn Option[T] @unwrap_or_else(f fn() -> T) -> T       // None → f() (lazy default)
fn Option[T] @map[U](f fn(T) -> U) -> Option[U]
fn Option[T] @ok_or[E](err E) -> Result[T, E]        // None → Err(err)
fn Option[T] @or(other Option[T]) -> Option[T]
```

**Базовые методы `Result[T, E]`:**

```nova
fn Result[T, E] @is_ok() -> bool
fn Result[T, E] @is_err() -> bool
fn Result[T, E] @ok() -> Option[T]                   // Ok(v) → Some(v); Err → None
fn Result[T, E] @err() -> Option[E]                  // Err(e) → Some(e); Ok → None
fn Result[T, E] @unwrap() Fail[E] -> T               // Err(e) → throw e
fn Result[T, E] @unwrap_or(default T) -> T           // Err → default
fn Result[T, E] @unwrap_or_else(f fn(E) -> T) -> T   // Err → f(e) (lazy)
fn Result[T, E] @map[U](f fn(T) -> U) -> Result[U, E]
fn Result[T, E] @map_err[F](f fn(E) -> F) -> Result[T, F]
```

`unwrap_or` / `unwrap_or_else` — основной идиоматический путь
безопасного доступа к значению с fallback. Прецеденты — Rust
`Option::unwrap_or`, Swift `??` оператор, TypeScript `??`.

```nova
let n int = parse_int(s).unwrap_or(0)               // на ошибке — 0
let cfg = config.unwrap_or_else(|| default_config())  // lazy default

// Идиома: цепочка через map / unwrap_or:
let port int = env.get("PORT").map(parse_int).unwrap_or(8080)
```

`@unwrap()` — assertion-style: throw'ает Fail если None/Err. Идиома
для случаев когда программист **гарантирует** что значение есть
(prove'ил выше через `if let` / `match`). Caller-side либо ловит
через `with Fail = ...`, либо позволяет распространиться (паника
на границе fiber'а — D13).

#### Bootstrap status (2026-05-08)

| Метод | Codegen | Тесты |
|---|---|---|
| `Option.is_some` / `is_none` | ✅ | ✅ |
| `Option.unwrap` (Fail на None) | ✅ inline | ✅ runtime/unwrap_or.nv |
| `Option.unwrap_or(default)` | ✅ runtime helper | ✅ |
| `Option.unwrap_or_else(f)` | ✅ inline (closure call) | ✅ runtime/result_methods.nv |
| `Option.map(f)` | ✅ inline | ✅ |
| `Option.ok_or(e)` | ✅ inline | ✅ |
| `Option.or(other)` | ❌ не реализован в bootstrap | — |
| `Result.is_ok` / `is_err` | ✅ | ✅ |
| `Result.ok()` → Option[T] | ✅ runtime helper | ✅ |
| `Result.err()` → Option[E] | ✅ inline (boxed nova_str) | ✅ |
| `Result.unwrap` (Fail на Err) | ✅ inline | ✅ |
| `Result.unwrap_or(default)` | ✅ runtime helper | ✅ |
| `Result.unwrap_or_else(f)` | ✅ inline (closure call) | ✅ |
| `Result.map(f)` | ✅ inline | ✅ |
| `Result.map_err(f)` | ✅ inline | ✅ |

**Bootstrap-ограничения**:
- `Result[T, E]` зашит на `(nova_int Ok, nova_str Err)`. Generic
  monomorphization для произвольных T/E — отдельная задача
  (Q-result-monomorphization).
- Lambda-параметры с не-`int` типом (например `(e str) -> str => ...`
  для `map_err`) требуют **явной аннотации** `(e str) -> str`. Codegen
  не делает inference closure-параметра по сигнатуре method'а
  (Q-closure-param-inference).
- Zero-arg lambda для `unwrap_or_else` — `() => expr` или
  `() -> T => expr`. Парсер lookahead за `(` теперь различает
  zero-arg lambda и unit-литерал `()`.

**Прочие prelude-типы:**

```nova
// Error — record для quick-and-dirty ошибок с сообщением (D65)
type Error {
    readonly msg str
}
fn Error.new(msg str) -> Error => { msg }

// RuntimeError — sum-тип встроенных runtime-сбоев (D65)
// Бросается встроенными операциями: a/b на 0, arr[i] на out-of-bounds, etc.
// StackOverflow и OutOfMemory не входят — они panic, не Fail (D13).
type RuntimeError
    | DivByZero
    | Overflow
    | IndexOutOfBounds { index int, length int }
    | TypeMismatch(str)
    | AssertFailed(str)
    | NoHandler(str)

// Iterator protocol (D58)
type Iter[T] protocol {
    mut next() -> Option[T]
}

// Range — литерал `a..b` / `a..=b` (D58)
type Range {
    readonly start int
    readonly end int
    readonly inclusive bool
}
type RangeIter {
    end       int
    inclusive bool
    mut cur   int
}
```

**Базовые числовые и строковые типы** (`int`, `i8`-`i64`, `u8`-`u64`,
`f32`, `f64`, `str`, `bool`, `char`, `()`, `byte`) — встроены в язык,
не stdlib, но упомянуты для полноты.

**`char` — Unicode codepoint, НЕ UTF-8 byte sequence.** `char` хранит
**одно скалярное значение Unicode** (диапазон 0..0x10FFFF, исключая
surrogate pairs 0xD800..0xDFFF). Размер в памяти — 4 байта (как Rust
`char`, Go `rune`, Swift `Unicode.Scalar`).

`str` хранит UTF-8 байты, `char` — codepoint. Конверсии:
- `char → str` или `char → []byte` — UTF-8 encode (1-4 байта в
  зависимости от значения; см. `Buffer.add_char` в Q-buffer).
- `str.chars() -> Iter[char]` — UTF-8 decode по ходу итерации.

Это разделение типичное для современных языков (Rust, Swift). Go
использует `rune` = `int32` по тому же принципу. C `char` это byte —
**не** аналог Nova `char`.

Bootstrap-status: `char` зарезервирован как тип, но синтаксис
char-литералов (`'a'`) — ещё открытый вопрос (Q-char-literals).
В коде сейчас используется `nova_int` напрямую (передаём codepoint
как число) — это будет заменено на нормальный `char` при закрытии
Q-char-literals.

**`str` — Unicode-string.** Внутреннее представление — UTF-8 байты
`(ptr, byte_len)`, но **все public operations работают на уровне
codepoint'ов** (Unicode scalar values). Содержимое — валидный UTF-8
по конвенции: литералы, конкатенация и `str.from(...)` гарантируют
валидность; FFI-код должен сам проверять при создании `str` из
чужого буфера.

**Длина и индексация (codepoint-indexed, школа Python/Swift):**

- `s.len` — длина в **codepoint'ах**, O(n) (требует обхода UTF-8).
  Это **базовая** «длина строки» с точки зрения программиста.
- `s.byte_len()` — длина в байтах, O(1). Для FFI и буферных операций.
- `s.slice(a, b)` принимает **codepoint-индексы**, O(b) (нужен обход
  до byte-offset'ов). Boundary всегда корректные — невозможно
  попасть в середину multi-byte sequence.
- `s[i]` (codepoint indexing) — `Option[char]`, O(i). `None` если
  `i >= s.len`. См. также Q-string-indexing.
- `s.chars() -> Iter[char]` — ленивый обход codepoint за codepoint.

**Поиск, сравнение, конверсия** (все индексы — **codepoint-offset**):

```nova
fn str @find(needle str) -> Option[int]          // codepoint-offset
fn str @rfind(needle str) -> Option[int]         // последний codepoint-offset
fn str @contains(needle str) -> bool
fn str @starts_with(prefix str) -> bool
fn str @ends_with(suffix str) -> bool
fn str @split(sep str) -> Iter[str]
fn str @trim() -> str
fn str @to_lower() -> str
fn str @to_upper() -> str
```

`s.find(":") -> Option[int]` возвращает **codepoint-индекс** ":".
Это передаётся напрямую в `s.slice(0, i)`:

```nova
let s = "Привет:мир"           // 10 codepoints, 19 bytes
let i = s.find(":").unwrap_or(0)  // i == 6 (codepoints)
let key = s.slice(0, i)        // "Привет"
let val = s.slice(i + 1, s.len)// "мир"
assert(s.len == 10)            // codepoints
assert(key.len == 6)
```

**Почему codepoint-indexing (школа B) выбрана для Nova:**

1. **AI-friendly.** LLM генерирует код где `s.len` интуитивно
   «количество символов». Byte-уровень (Rust/Go) — источник bug'ов
   у новичков и AI: `"Привет".len == 12` нелогично.
2. **Безопасность boundary.** Невозможно попасть в середину UTF-8
   sequence — все индексы codepoint-выровнены.
3. **Consistency.** `find` / `slice` / `s[i]` — все codepoint-уровень,
   не нужно мысленно переключаться между byte и codepoint.
4. **Прецеденты:** Python (codepoints), Swift (graphemes — ещё выше),
   Java (UTF-16 code units, близко к codepoint для BMP). Все
   современные языки кроме system-low-level (Rust, Go, C) выбирают
   codepoint-or-grapheme уровень.

**Цена:**

- O(n) для `s.len`, O(b) для `s.slice(a, b)` — обходы UTF-8.
  Внутреннее byte-хранилище неизбежно: альтернатива (UTF-32 4-byte
  per char) утроит память для ASCII-heavy кода.
- Hot-path работа с byte-уровнем — через explicit `s.bytes()`
  → `[]byte` или через `Buffer` (Q-buffer).
- В Nova принципе AI-генерация важнее микро-perf для primitive ops;
  программист может явно перейти на byte-уровень там где надо.

**FFI / byte-уровень доступен через:**

```nova
fn str @byte_len() -> int                    // O(1) — для C-interop размеров
fn str @bytes() -> []byte                    // copy (D73 []byte.from(s))
```

**Конверсия в `[]byte` через D73:**
- `[]byte.from(s str) -> []byte` — infallible (всегда работает,
  `str` гарантированно валидный UTF-8). **Копирует**
  `s.ptr..s.ptr+s.len` в свежий `[]byte`. D73 авто-синтезирует
  `s.into()` для `let b []byte = s.into()`.
- Копирует, не view: Nova не имеет readonly-меток (D6 — managed
  heap без borrow-checker), а `[]byte` mutable — без копии mutate
  испортил бы immutability `str`. Стоимость O(n) — приемлемо для
  границы str↔bytes; для in-place аккумуляции использовать `Buffer`
  (Q-buffer).
- `str.from(b []byte) Fail[Utf8Error] -> str` — fallible-форма
  (D73 + Fail-effect). Валидирует UTF-8; на ошибке throw'ает.
  Auto-derived: `b.into()` тоже декларирует `Fail[Utf8Error]`.
  Result-форма (`str.try_from(b)` → `Result[str, Utf8Error]`)
  доступна через D77 как convenience sugar.

**Nul-termination (C-interop):** `nova_str_concat` сейчас аллоцирует
`len + 1` байт и кладёт `\0` после данных, чтобы `s.ptr` можно было
передать в C-функции. Литералы тоже nul-terminated (`.rodata` C-string).
Slice — **НЕ** добавляет `\0` (просто view). Это значит
`nova_str.ptr` — **не** гарантированно cstring; зависит от того как
строка построена. **Открытый вопрос (Q-cstring):** либо унифицировать
("все `nova_str` всегда nul-terminated, slice копирует") ценой
аллокаций, либо отказаться от частичной гарантии и ввести явный
`s.as_cstr() -> *const char` (с копированием при необходимости).
В bootstrap'е действует текущее inconsistent поведение.

**Дедупликация / interning:** `str` **не интернируется автоматически**.
Одинаковые runtime-строки — разные инстансы. `==` сравнивает контент
(memcmp), O(min). Compile-time литералы deduplicate-аются C-компилятором
через стандартное string-literal pooling в `.rodata`. Для opt-in
interning — **открытый вопрос (Q-string-interning):** Atom-тип или
`Sym[T]` (Erlang-style); прецеденты — Rust не интернирует, Java/C#
имеют пул для литералов + opt-in `intern()`.

**Конкатенация:** `s1 + s2` — O(a+b), новая аллокация каждый раз.
В hot loop `s = s + x` × N → O(N²). Для аккумуляции использовать
**`Buffer`** (Q-buffer; финализация через `@try_into() -> Result[str,
Utf8Error]` для UTF-8 или `@into() -> []byte` для сырых данных).
Nova унифицирует string-builder и byte-buffer в один тип — отличается
от Go (`bytes.Buffer` + `strings.Builder`) и Rust (`Vec<u8>` +
`String`).

См. также [Q-char-literals](../open-questions.md) (синтаксис
char-литералов) и [D54](03-syntax.md#d54) (`as`/`is` для конверсий).

**Математические операции на числовых типах** объявлены как
**instance-методы** через `@` ([D74](#d74-математические-операции-на-числовых-типах--instance-методы)):
`x.sqrt()`, `theta.cos()`, `y.atan2(x)`, `a.hypot(b)`, `n.abs()`,
`x.is_finite()`, etc. Static-функции — только для констант
(`f64.PI`, `f64.NAN`) и парсинга (`f64.try_parse(s)`).

**`any`** — пустой protocol-тип (D53). Любой тип удовлетворяет
пустому контракту, поэтому `any` — top-type (универсальный супертип).
Имя lowercase — исключение в [03-syntax.md → D30](03-syntax.md#d30)
naming convention, по аналогии с примитивами. Использование:
`fn dump(x any) Io -> ()`, `Logger.log_event(level, fields []any)`
для гетерогенных структурных логов.

**`Iter[T]`** — структурный protocol для итераторов (D58). Любой
тип с методом `mut next() -> Option[T]` автоматически удовлетворяет.
`for x in collection`-синтаксис вызывает `collection.iter().next()` в
цикле; коллекции реализуют `iter()` возвращая собственный iterator-тип.

**`Range`** — runtime-представление range-литерала `a..b` (exclusive)
и `a..=b` (inclusive) (D58). Range — обычное значение, можно
передавать как аргумент, хранить в переменной, использовать в `for`.

**Стандартные эффекты** в prelude — после [D62](04-effects.md#d62)
делятся на **две категории** по влиянию на семантику программы:

#### Semantic effects — влияют на результат

Программист **обязан** объявить в сигнатуре, если функция их
использует. Caller получает информацию что зависит от resource'а.

| Эффект | Resource | Тестовый handler |
|---|---|---|
| `Fail[E]` | error reporter | `with Fail[E] = (e) => ...` |
| `Io` | stdout/stderr | mock-stdout |
| `Net` | сеть (HTTP/socket) | recorded responses |
| `Db` | соединение к БД | in-memory db |
| `Fs` | файловая система | virtual-fs |
| `Time` | clock | `fixed_ms(ms)` |
| `Random` | RNG | `seeded(seed)` |
| `Log` | logger | capture-log |
| `Ask[T]` | контекстный read (Reader) | fixed value |
| `Alloc[R]` | region аллокация | (для real-time, [D6](05-memory.md#d6)) |
| `Detach` | background scheduler | `SyncDetach` |
| `Blocking` | OS-thread pool | mock |

#### Instrumental effects — observability, ambient

`Mem` ([D76](#d76)) и `Trace` — **не влияют** на результат программы,
только на наблюдаемость. Программист **не декларирует** их в
сигнатуре; компилятор не лифтит через D28-inference.

```nova
// Программист пишет:
fn parse_data(s str) -> Data { ... }

// Внутри может быть Trace.span("parse"), Mem.alloc_count() — это
// implementation detail, в сигнатуру НЕ лифтится.
```

**Ambient capability — прецедент `Async` (D14/D62).** Если в скоупе
нет active handler для instrumental эффекта — runtime-panic
(`RuntimeError.NoHandler("Mem")` через [D65](04-effects.md#d65)),
**не compile error**.

| Эффект | Категория |
|---|---|
| `Mem` | instrumental, ambient |
| `Trace` | instrumental, ambient |

**Зачем разделять:**

1. **Сигнатуры остаются чистыми.** Если бы `Trace` был semantic, то
   почти **каждая** функция бы содержала его — observability обычно
   pervasive. Шум в типах.
2. **AI-friendly.** LLM не должна писать `Mem` в сигнатуре —
   instrumental detail имплементации.
3. **Интуитивно.** `Time` в сигнатуре говорит "функция зависит от
   времени, тестируй с fixed clock". `Trace` в сигнатуре ничего
   полезного не говорит.

#### Не существуют как эффекты

| Имя | Почему |
|---|---|
| `Async` | runtime mechanic (suspension, [D14 (REVISED)](06-concurrency.md#d14)) |
| `Par` | runtime mechanic (parallelism через `parallel for`) |
| `Mut` | удалён ([D62](04-effects.md#d62)) — `mut` поля/параметры |

**Базовые функции:**

```nova
fn print(...items []any) Io -> ()           // variadic, см. D69
fn println(...items []any) Io -> ()         // variadic + newline
fn panic(msg str) -> Never

// Assertions — обычные fn-call, обязательно со скобками
fn assert(cond bool) -> ()                  // always runtime; failure → panic (D13)
fn debug_assert(cond bool) -> ()            // debug-only; no-op в release (D81)
```

`print`/`println` — **variadic** ([D69](03-syntax.md#d69)),
принимают любое число аргументов любого типа (`any` —
[D54](03-syntax.md#d54)). Каждый аргумент конвертируется в строку
через `str.from(v)` ([D73](#d73-from--into-protocol-пара-с-авто-выводом)).
Spread разрешён: `print(...parts)`.

`assert`/`debug_assert` — **обычные функции, не keyword'ы**. Вызываются
со скобками как любой fn-call: `assert(x > 0)`. Build-mode семантика —
[D81](#d81). Failure любого assert'а — panic ([D13](#d13)), не Fail.

#### `Never` — обычный тип без значений

`Never` объявлен как **sum-type с нулём вариантов** — синтаксически
`type Never =` (после `=` пусто). Это легальная конструкция в системе
[02-types.md → D17](02-types.md#d17): пустой список вариантов —
корректный частный случай.

**Свойства следуют из пустоты, не из специального правила:**

- **Нельзя создать значение типа `Never`** — нет ни одного варианта.
- **`Never` — подтип любого типа** (bottom type ⊥). Любой контекст,
  ожидающий `T`, может принять `Never`-выражение.
- **Используется в типах не-возвращающих выражений** — `throw expr`,
  `return expr`, `panic(...)`, бесконечный `loop`. Все имеют тип
  `Never`, поэтому совместимы с любым контекстом.

Аналоги: Rust `!`, Haskell `Void`, Kotlin/Scala `Nothing`,
TypeScript `never`. Не уникальная фича Nova.

#### Эффекты как обычные типы — `Fail[E]` не магия

`Fail[E]` объявляется в prelude как любой другой эффект — через
kind-токен `effect` ([04-effects.md → D18 (REVISED)](04-effects.md#d18-эффекты-объявляются-через-kind-токен-не-голый-type),
[D61](04-effects.md#d61)):

```nova
type Fail[E] effect {
    fail(value E) -> Never
}
```

`throw expr` — сахар для `Fail[E].fail(expr)` (вызов операции
активного handler'а), как `Db.query(...)`. Никакой специальной
обработки. См. [04-effects.md → D25](04-effects.md#d25),
[04-effects.md → D61](04-effects.md#d61).

#### Что НЕ в prelude

Коллекции (`String`, `HashMap`, `HashSet`, `LinkedList`), I/O API (`File`, `Http`),
JSON, SQL, время как библиотека — **обычные модули**, требующие
явного импорта:

```nova
import std.io.{File, read_all}
import std.collections.HashMap
```

### Почему

#### Зачем нужен prelude

Без prelude каждый файл начинается с:

```nova
import std.option.{Option, Some, None}
import std.result.{Result, Ok, Err}
```

Это шум на 90% файлов. Прецедент — Rust, Haskell, Swift, Kotlin: все
имеют prelude. AI-first: LLM не должен генерировать boilerplate-импорты
базовых типов.

#### Не противоречит «локальности контекста»

Prelude **документирован**, его содержимое — фиксированный список,
не магия. LLM знает, что доступно везде. Всё остальное — явный импорт
([07-modules.md → D29](07-modules.md#d29)).

### Что отвергнуто

- **Никакого prelude, всё через явный import** — шум, не выигрыш.
- **Prelude определяется компилятором, без документации** — магия,
  ломает AI-first тезис.
- **Prelude настраивается per-project** — усложнение без выгоды; LLM
  должен знать фиксированный набор.
- **`Void`** — отвергнут, тип «без значения» это `()` (unit). См.
  [03-syntax.md → D20](03-syntax.md#d20).

### Связь

- [01-philosophy.md → D10](01-philosophy.md#d10) — AI-first,
  локальность через документированный prelude.
- [04-effects.md → D25](04-effects.md#d25) — `throw` и `Fail[Error]`.
- [04-effects.md → D18](04-effects.md#d18) — эффекты как обычные типы.
- [02-types.md → D17](02-types.md#d17) — sum-type, `Never` как пустой.
- [03-syntax.md → D20](03-syntax.md#d20) — `()` вместо `void`.
- [07-modules.md → D29](07-modules.md#d29) — prelude и явные импорты.

### Открытые вопросы

- ~~Полный API `Option`/`Result`~~ — **частично закрыт (2026-05-07):**
  базовые методы (`is_some`/`is_none`/`unwrap`/`unwrap_or`/`unwrap_or_else`/
  `map`/`ok_or`/`or` для Option; `is_ok`/`is_err`/`ok`/`err`/`unwrap`/
  `unwrap_or`/`unwrap_or_else`/`map`/`map_err` для Result) описаны в
  prelude выше. Расширенный API (`and_then`, `flatten`, etc.) —
  отдельная задача (Q-monadic-api).
- ~~Семантика `?` для `Option`~~ — закрыто
  [D67](04-effects.md#d67): ранний `return None` из текущей функции.
- `Error` как универсальный тип — что в нём (поддержка `str.from(e)`,
  цепочка причин)? Похоже на Rust `std::error::Error`.

### Цена

1. **Список prelude нужно поддерживать.** Любое добавление в prelude —
   breaking change после v1.0 (имя становится «зарезервированным» в
   модулях). Поэтому prelude **минимален**.
2. **Импорт-конфликты.** Если программист объявит свой `type Option`,
   будет конфликт с prelude — компилятор предупредит.

---

## D41. Static-функции есть, static-состояния нет

### Что
У типа есть **static-функции** (`fn Type.name(...)`), но **нет
static-полей**, **нет static-переменных**, **нет static initializer'ов**.
Если нужны константы, ассоциированные с типом, — это `const` в том же
модуле. Если нужно «глобальное» изменяемое состояние — это **handler**
(эффект-capability), не static.

### Правило

#### Static-функции — обычные функции в namespace типа

Внутри одной static-функции другие static-функции того же типа
вызываются **через полное имя**, без сокращений:

```nova
fn Account.new(owner str) -> Account =>
    Account { _balance: 0, owner }

fn Account.from_balance(owner str, initial money) -> Account {
    let acc = Account.new(owner)             // явное Account.new, не self.new
    Account.deposit_static(acc, initial)     // тоже явно
    acc
}
```

Никакого `Self::new` (Rust) или просто `new` (Java/C#). Один способ
вызова static-функции — через имя типа, что внутри типа, что снаружи.

#### Константы рядом с типом — `const` в модуле

```nova
const ACCOUNT_MIN_BALANCE money = 0
const ACCOUNT_MAX_OVERDRAFT money = 1000

fn Account.new(owner str) -> Account =>
    Account { _balance: ACCOUNT_MIN_BALANCE, owner }
```

Если нужна группировка — отдельный модуль:

```nova
module account_limits

export const MIN_BALANCE money = 0
export const MAX_OVERDRAFT money = 1000

// использование:
import account_limits
let acc = Account.new_with(account_limits.MIN_BALANCE)
```

#### Глобальное изменяемое — через handler

Вместо static counter / static config — handler, передаваемый через
`with`-блок:

```nova
// Эффект ([04-effects.md → D61](04-effects.md#d61))
type IdGen effect {
    fresh() -> u64
}

// Handler — обычная функция, возвращающая handler-литерал
fn counter_id_gen(c mut Counter) -> Handler[IdGen] =>
    handler IdGen {
        fresh() {
            c.count += 1
            c.count
        }
    }

// в main:
fn main() {
    let mut counter = Counter { count: 0 }
    with IdGen = counter_id_gen(counter) {
        run_app()
    }
}
```

> Это пример **closure-capture** паттерна по [D68](04-effects.md#d68).
> Альтернатива — `@as_handler` метод на record'е `Counter` —
> рассмотрена в D68 для случаев, когда state нужно проинспектировать
> снаружи. Выбор между паттернами детерминирован сценарием
> (нужен ли state наружу), не вкусом.

Тестируется тривиально — другой handler в `with`-блоке.

### Почему

- **Static state — главный источник скрытых багов.** Глобальный
  изменяемый стейт не виден в сигнатурах, ломает параллельность,
  невозможно тестировать без хаков.
- **Тесты.** Static-поле = разделяемое состояние между тестами.
  Каждый тест должен либо ресетить его (хрупко), либо запускаться
  изолированно (медленно). Handler — `with`-блок изолирует
  автоматически.
- **Параллелизм.** Несколько fiber'ов на одном static-поле = data race
  по умолчанию. Handler-state живёт в scope и не делится случайно.
- **DI is the language.** Передача зависимостей — это handler. Не
  нужен отдельный фреймворк для DI, не нужны static-singleton'ы как
  замена.
- **Единственный путь.** Нет «иногда static, иногда handler» —
  всегда handler. Меньше способов сделать неправильно.

### Что отвергнуто

- **Static mutable поля** (Java `static int counter`, Python class
  variable) — мешают тестам и параллелизму.
- **Static immutable поля как `const`** на типе (`const Account.MIN`)
  — технически безопасно, но добавляет второй способ объявить
  константу. Один способ — `const` в модуле.
- **Companion-object** (Kotlin) — то же что и static, просто в
  обёртке. Не нужен.
- **Lazy static** (Rust `lazy_static!`) — скрытое глобальное состояние
  с инициализацией. Если нужна ленивость — handler с lazy полем.

### Связь

- [05-memory.md → D6](05-memory.md#d6) — глобального mutable state не
  предусмотрено в модели памяти; всё живёт в fiber-scope или
  handler-scope.
- [04-effects.md → D11](04-effects.md#d11),
  [04-effects.md → D31](04-effects.md#d31) — handler-механизм для
  «глобальных» состояний.
- [04-effects.md → D18](04-effects.md#d18) — эффекты это обычные `type`,
  не keyword `effect`.
- [03-syntax.md → D33](03-syntax.md#d33) — `const` — единственный
  способ объявить immutable «глобальную» константу.

### Цена

1. **Привычка из Java/C#/Python ломается.** Нет `Account.MAX_BALANCE`
   как поля, есть `MAX_BALANCE` как `const` в модуле. Чуть длиннее,
   но единообразнее.
2. **Singleton'ы переписываются как handler.** Это не цена, а фича —
   но мигрирующий код придётся переделать.
3. **Counter / cache / pool** требуют явного создания и проброса в
   `with`-блок. Не «само работает», а явный жизненный цикл.

### Эволюция

В исходной формулировке D41 пример использовал устаревшие keyword'ы
`effect IdGen { ... }` и `handler counter_id_gen(...) IdGen { ... }` —
оба отменены ([04-effects.md → D18](04-effects.md#d18) — эффект это
обычный `type`; слово `handler` не зарезервировано).
В текущем тексте пример переписан как `type IdGen { ... }` +
обычная функция, возвращающая handler-литерал.

---

## D70. `ToStr` protocol + `@to_str()` метод + free function `to_str(v)`

> ⚠️ **REPLACED → [D73](#d73-from--into-protocol-пара-с-авто-выводом).**
> `ToStr` отменён как отдельный protocol — конверсия в строку это
> частный случай `From`/`Into`-механизма из D73. Вместо `@to_str()` /
> `ToStr` пишется `fn str.from(v X) -> Self` (или `fn X @into() -> str`),
> и компилятор автоматически даёт обе формы вызова: `str.from(v)`
> и `v.into()`. String interpolation `"${v}"` использует `str.from(v)`
> внутри. См. D73 для полной семантики и [«Эволюция»](#d70-эволюция)
> ниже про переход.
>
> Раздел сохранён как историческая справка; в живом коде D70-механизм
> не используется.

### Что
Универсальный механизм конверсии значения в строку:

1. **`ToStr`** — protocol с одним методом `@to_str() -> str`.
2. **`@to_str()`** — метод на типе, реализует представление в строку.
3. **`to_str(v)`** — свободная функция в prelude, sugar над `v.to_str()`.

Все встроенные типы (`int`, `str`, `bool`, `float`, `()`,
record/sum-комбинации) реализуют `ToStr` автоматически (auto-derive
по структуре). Программист может override на своих типах через обычный
`@`-метод.

### Правило

#### Декларация protocol'а в prelude

```nova
type ToStr protocol {
    to_str() -> str
}
```

#### Builtin реализации (auto-derive)

Все базовые типы реализуют `ToStr` автоматически — программист **не
пишет** `@to_str()` для:

| Тип | Формат |
|---|---|
| `int` (любой size) | десятичное число: `42`, `-100` |
| `float` (f32/f64) | как Rust `Display`: `3.14`, `-0.5` |
| `bool` | `true` / `false` |
| `str` | сама строка (без кавычек) |
| `()` (unit) | `()` |
| `[]T` (где T: ToStr) | `[a, b, c]` (элементы через `to_str`) |
| `(A, B, ...)` tuple | `(a, b, ...)` |
| record `T { f1, f2 }` | `T { f1: ..., f2: ... }` |
| sum-variant `Foo(x)` | `Foo(x)` |
| sum-variant `Bar` (unit) | `Bar` |

Auto-derive работает рекурсивно — записи и sum-варианты
форматируются через `to_str()` своих полей/аргументов.

#### Override на пользовательском типе

```nova
type UserId u64

fn UserId @to_str() -> str => "user#${@}"

let id = UserId(42)
to_str(id)              // "user#42" (через override)
"id is ${id}"           // "id is user#42" (string interpolation также через ToStr)
```

#### Free function `to_str`

```nova
fn to_str[T: ToStr](v T) -> str => v.to_str()
```

Это единственная универсальная точка для получения строкового
представления. Внутри `print`/`println` и string interpolation
используется именно `to_str(v)`.

#### Compile-time enforcement

`ToStr`-bound — обычный generic-bound:

```nova
fn debug_log[T: ToStr](label str, v T) Log -> () =>
    Log.info("${label} = ${to_str(v)}")
```

Если программист объявил `type MyType { ... }` и НЕ реализовал
`@to_str()`, и тип не подпадает под auto-derive — `to_str(my)`
вызовет compile error «`MyType` does not implement `ToStr`».

В практике auto-derive покрывает большинство случаев, поэтому
явное объявление `@to_str()` нужно только для **кастомного формата**
(как `UserId` выше).

#### Связь со string interpolation

Любой `${expr}` в string-литерале — sugar над `to_str(expr)`:

```nova
"id=${user_id}"          // ≡ "id=" + to_str(user_id)
"point=(${x}, ${y})"     // → "point=(3, 4)"
```

Тип `expr` должен реализовывать `ToStr` (обычно auto-derive).

### Семантика auto-derive

Компилятор генерирует **default `@to_str()`** для:

- **Record**: `T { f1: v1, f2: v2 }` → `"T { f1: ${to_str(v1)}, f2: ${to_str(v2)} }"`
  - Поля выводятся в порядке объявления (D52).
- **Sum-variant**: `Foo(x, y)` → `"Foo(${to_str(x)}, ${to_str(y)})"`
- **Sum-unit-variant**: `Red` → `"Red"`
- **Tuple**: `(a, b, c)` → `"(${to_str(a)}, ${to_str(b)}, ${to_str(c)})"`
- **Array**: `[a, b, c]` → `"[${to_str(a)}, ${to_str(b)}, ${to_str(c)}]"`
- **Newtype**: тот же что и underlying — `type UserId u64` без override
  → `to_str(UserId(42))` = `"42"`. Override меняет.

Все элементы рекурсивно требуют `ToStr`. Если хоть один не реализует —
compile error на месте использования.

### Почему

1. **AI-friendly default** — программист пишет `to_str(v)` или `"${v}"`
   и получает работу для любого типа. Не нужно реализовывать `Show`-
   trait вручную.

2. **Compile-time enforcement** — `ToStr`-bound в функциях
   (`fn f[T: ToStr]`) даёт явный контракт. LLM/compiler ловит
   несоответствие до runtime'а.

3. **Override через стандартный `@`-метод** — не новый синтаксис.
   Если auto-derive формат не подходит — пишешь `fn T @to_str()` как
   обычный метод.

4. **Один protocol, не два** (как Rust `Display`/`Debug`) — D40
   «один способ». Если когда-то понадобится debug-формат — отдельный
   D-блок (`Debug` protocol с `@to_debug()`), но не сейчас.

5. **Имя `ToStr` буквальное** — описывает что делает (converts to
   `str`). Не путается с UI-кодом (как `Display`/`Show`).

6. **Symmetric с возможным расширением:**
   - `ToStr` → `to_str() -> str`
   - `ToJson` (если понадобится) → `to_json() -> Json`
   - `ToBytes` → `to_bytes() -> []u8`

   Единое naming convention.

### Что отвергнуто

- **`Display` имя** (как Rust). Слишком общее, конфликтует с UI/HTML
  кодом (`fn Slide @display()`). `ToStr` описательнее.
- **`Show` имя** (Haskell/OCaml). Конфликтует с UI (`popup.show()`).
- **`Stringer` имя** (Go). Метод в Go называется `String()`; у нас
  метод `to_str()` — несоответствие.
- **Без protocol'а, только free function `to_str(any)`**. Без bound'а
  нет compile-time enforcement; программист может забыть реализовать
  override и получит auto-derive вместо ожидаемого формата.
- **Два protocol'а `ToStr` + `Debug`** (как Rust). У Nova нет
  отдельной debug-семантики на уровне prelude. Если понадобится —
  отдельный D-блок.
- **Универсальный `@cast[X]` метод** (был рассмотрен и отвергнут):
  - `[X]` синтаксически объявляет generic-параметр (D16), не target —
    конфликт грамматики.
  - Return-type dispatch требует typeclass-механизма, которого в Nova
    пока нет.
  - Каждая конверсия — отдельный protocol с уникальным именем
    (`ToStr`, `ToJson`) — D46 overloading по имени работает естественно.

### Связь

- [D26](#d26-базовая-stdlib-и-prelude) — `to_str(v)` в prelude,
  `print`/`println` через variadic ([D69](03-syntax.md#d69)).
- [D35](03-syntax.md#d35) — `@`-методы.
- [D40](01-philosophy.md#d40) — «один способ» (один protocol, не два).
- [D42 (REVISED)](02-types.md#d42) / [D53](02-types.md#d53) /
  [D62](04-effects.md#d62) — `protocol` для структурных контрактов.
- [D46](03-syntax.md#d46) — overloading методов по имени.
- [D69](03-syntax.md#d69) — variadic `print(...items []any)` использует
  `to_str` для каждого элемента.

### Эволюция

В bootstrap-stdlib функция `to_str(v)` существовала как Native-функция,
работающая на любом значении через Rust-side `format!("{}", v)` (то
есть auto-derive прямо на runtime-уровне). Но **формальной декларации
`ToStr` protocol'а в спеке не было** — это был implementation-факт.

D70 формализует:
1. `ToStr` protocol с методом `@to_str()` — стандартная декларация.
2. Auto-derive для всех встроенных + record/sum типов.
3. Override через обычный `@to_str()` метод.
4. Free function `to_str[T: ToStr](v T) -> str` — публичный API.
5. String interpolation `"${expr}"` — sugar над `to_str(expr)`.

Альтернативы рассмотрены и отвергнуты:
- `Display`/`Show`/`Stringer` имена — конфликты с UI-кодом или
  inconsistency с именем метода.
- Универсальный `@cast[X]` — синтаксический конфликт с generic-
  параметрами и нет return-type dispatch'а в Nova.
- Без protocol'а — нет compile-time enforcement.

#### v3 (2026-05-06) — REPLACED → D73

D70 отменён как отдельный механизм. Конверсия в строку — частный
случай универсального `From`/`Into`-механизма из D73:

| Старая форма (D70) | Новая форма (D73) |
|---|---|
| `fn UserId @to_str() -> str => ...` | `fn str.from(u UserId) -> Self => ...` |
| `to_str(user)` | `str.from(user)` |
| `user.@to_str()` | `user.into()` (`Into[str]` авто-выведен из `From`) |
| `"${user}"` (через `to_str`) | `"${user}"` (через `str.from`) |

**Почему замена сделана:**

1. **Дублирование механизмов.** D70 + D73 решают **одну задачу**
   («конверсия значения в другой тип») разными способами.
   Конверсия в `str` — частный случай конверсии в любой тип.
2. **Принцип «один очевидный путь» (D9).** Программист не должен
   выбирать между `to_str` и `into[str]` для одного и того же.
3. **Methods on primitives (D35).** Расширение D35 явно позволяет
   `fn str.from(...)` — раньше это было неочевидно. С этим
   `str.from` становится естественным конструктором.
4. **AI-friendly.** LLM генерирует `str.from(x)` единообразно с
   любой другой конверсией, без специального правила «для строк
   используй to_str».

**Как мигрировать:** заменить `@to_str() -> str` на `str.from(v Self)`
(switching method body to `static-method-on-str`), либо `@into() -> str`
(оставить body на receiver-типе). Free function `to_str(v)` —
вызовы заменяются на `str.from(v)`. String interpolation работает
автоматически (компилятор использует `str.from`).

**Auto-derive для встроенных типов и record/sum** — переносится из
D70 на `str.from`: stdlib pre-registers `str.from(int)`, `str.from(bool)`,
`str.from(f64)`, `str.from(<any record>)`, `str.from(<any sum>)` — те
же типы что в D70 авто-derive'ились.

---

## D73. `From` / `Into` protocol-пара с авто-выводом

> **Уточнение (2026-05-07):** `from`/`into` могут декларировать
> `Fail[E]` если конверсия fallible. Это **унифицирует** infallible и
> fallible конверсии под одной формой `from`/`into` — нет нужды в
> отдельном `try_from`/`try_into` (D77 теперь convenience-sugar,
> см. там).

### Что
Универсальный механизм нетривиальной конверсии значения между типами:

1. **`From[T]`** — protocol со static-методом `from(v T) -> Self`.
   «Целевой тип знает, как сделать себя из источника».
2. **`Into[T]`** — protocol с instance-методом `@into() -> T`.
   «Источник знает, как превратиться в целевой».
3. **Авто-вывод одного из другого** — компилятор знает про симметрию.
   Если задан только `From[X]` для типа `T`, компилятор автоматически
   удовлетворяет `Into[T]` для `X` (и наоборот). Программист пишет
   **одну** реализацию из пары.
4. **Fallible конверсии** объявляются эффектом `Fail[E]` в сигнатуре —
   та же `from`/`into` форма; effect-aware auto-derive переносит
   эффект на парную форму.

Программисту доступны **две формы вызова** из одной реализации:

```nova
T.from(v X)             // static, на целевом типе
v.into()               // instance, на источнике (тип цели — из контекста)
```

Для fallible (с `Fail[E]`) семантика та же; ошибка распространяется
через стандартный effect-механизм — `with Fail = handler { ... }` /
`?` оператор / propagation наружу.

В отличие от `as` (D54) — compile-time numeric/newtype/sum cast без
runtime-кода, — `From`/`Into` для **семантически нетривиальных**
конверсий (парсинг, единицы измерения, формат-обмен, представление
в строку — последнее заменяет old D70 `ToStr`).

### Правило

#### Декларация protocol'ов в prelude

```nova
type From[T] protocol {
    from(v T) -> Self           // static, на целевом типе
}

type Into[T] protocol {
    @into() -> T                 // instance, на источнике
}
```

`Self` (D66) — тип, реализующий protocol. `From.from` — static-метод,
вызывается через точку (D35): `Fahrenheit.from(celsius)`. `Into.@into`
— instance-метод, через `@`-нотацию: `c.into()`.

**Программист пишет одну сторону пары** — компилятор автоматически
выводит другую. Подробности — секция «`Into[T]` protocol и
автоматический вывод» ниже.

#### Реализация на пользовательском типе

Программист пишет обычный static-метод (D35):

```nova
type Celsius f64
type Fahrenheit f64

fn Fahrenheit.from(c Celsius) -> Self =>
    Self((c as f64) * 9.0 / 5.0 + 32.0)

let f = Fahrenheit.from(Celsius(100.0))   // Fahrenheit(212.0)
```

Структурно `Fahrenheit` теперь удовлетворяет `From[Celsius]` (D53 +
D72) — никаких явных `impl` блоков.

**Несколько `From[X]` на одном типе** через overloading по
параметру (D46):

```nova
fn Fahrenheit.from(c Celsius) -> Self => ...
fn Fahrenheit.from(k Kelvin) -> Self => ...

let f1 = Fahrenheit.from(Celsius(100.0))
let f2 = Fahrenheit.from(Kelvin(373.15))
```

#### Generic-функции с `From`-bound

```nova
fn parse_typed[U From[str]](s str) -> U => U.from(s)

let n int = parse_typed("42")     // если int реализует From[str]
```

Bound `[U From[X]]` в generic-сигнатуре требует чтобы конкретный
тип `U` реализовывал `From[X]` — структурно, через D72 bound check.

#### Fallible конверсии через `Fail[E]`

Если конверсия может **не получиться** (валидация, парсинг, проверка
диапазона), `from`/`into` декларируют `Fail[E]` в сигнатуре:

```nova
type Utf8Error | InvalidByte | UnexpectedEnd

fn str.from(b []byte) Fail[Utf8Error] -> Self {
    if !is_valid_utf8(b) {
        throw Utf8Error.InvalidByte
    }
    // ...
}

// Caller-side — три варианта:

// (1) Propagate via Fail в сигнатуре caller'а:
fn parse_message(b []byte) Fail[Utf8Error] -> Message {
    let s = str.from(b)              // ошибка пробрасывается
    parse_inner(s)
}

// (2) Catch handler'ом — Result-стиль через with-handler:
let r Result[str, Utf8Error] =
    with Fail[Utf8Error] = (e) => interrupt Err(e) {
        Ok(str.from(b))
    }

// (3) Default-fallback через with-handler:
let s str = with Fail[Utf8Error] = (_) => interrupt "[invalid utf-8]" {
    str.from(b)
}
```

**Effect-aware auto-derive:** если `T.from(v V) Fail[E] -> Self`,
компилятор авто-синтезирует `v.into() Fail[E] -> T`. Эффект
наследуется, видим в сигнатуре auto-derived формы.

#### Auto-derive 4-way (D73 + D77 unified)

**Программист пишет ОДНУ форму** из четырёх; компилятор синтезирует
остальные. Это объединяет D73 (`from`/`into`) и D77 (`try_from`/`try_into`)
в один механизм.

**Разделение «реализовать» vs «использовать»:**

| Природа конверсии | Программисту реализовать | Программисту использовать |
|---|---|---|
| **Fallible** | `T.try_from(v) -> Result[T, E]` | `T.from(v)` или `v.into()` (короче, throws Fail) |
| **Infallible** | `T.from(v) -> T` | `T.from(v)` или `v.into()` |

То есть **писать богатую форму** (`try_from` для fallible — Result-стиль
явный, error type first-class), а **использовать в обычном коде**
короткую (`from` / `into`).

**Compiler синтезирует все 4 формы из одной:**

| Программист написал | Compiler даёт |
|---|---|
| `try_from(v) -> Result[T, E]` (fallible) | `from() Fail[E]`, `into() Fail[E]`, `try_into() -> Result[T, E]` |
| `from(v) -> T` (infallible) | `into() -> T`. (try-формы НЕ синтезируются — не имеют смысла без error type.) |

**Почему `try_from` — самое богатое для имплементации:**
1. **Result в типе явный.** `Result[T, E]` показывает error type как
   first-class signature element — IDE / AI читают это сразу. Через
   `Fail[E]` нужен ещё шаг effect-rezolution.
2. **Compiler легко синтезирует throwing-форму** из Result — простое
   `match { Ok(v) => v, Err(e) => throw e }`. Обратное (Result из
   throwing) требует with-handler инфраструктуры.
3. **Boilerplate Ok(...) — это feature имплементации.** `Ok(value)`
   явно говорит «вот success-path», `Err(...)` — «вот failure-path».
   Программист читает контракт без неявных throw'ов в теле функции.

**Почему `from`/`into` — для использования в коде:**
1. **Короче** — `T.from(v)` против `T.try_from(v)?` или
   `T.try_from(v).unwrap()`.
2. **Идиоматичнее** — `v.into()` через context-driven dispatch
   читается как «преобразовать v к ожидаемому типу».
3. **Throws пропагируются естественно** — caller или handle через
   `with Fail`, или эффект уходит наружу. Программист не пишет
   `?`-цепочки руками.

**Когда использовать `try_from`/`try_into` в коде:**
- Когда нужен **explicit branching** на error type через `match`.
- Когда нужно **map error** в другой тип (`r.map_err(|e| MyError::Wrap(e))`).
- Когда нужен **default fallback** через `unwrap_or` без handler-блока.

В остальных случаях — `from`/`into` через эффекты.

**Прецедент Rust:** `TryFrom` каноническая форма для fallible
конверсий; сообщество выработало этот стиль.

**Алгоритм синтеза (программист пишет `try_from`):**

```nova
// Программист написал:
fn u64.try_from(s str) -> Result[Self, ParseIntError] => ...

// Компилятор синтезирует автоматически:
// (1) throwing-from через D73:
fn u64.from(s str) Fail[ParseIntError] -> Self =>
    match try_from(s) { Ok(n) => n, Err(e) => throw e }

// (2) instance try_into через D77:
fn str @try_into() -> Result[u64, ParseIntError] =>
    u64.try_from(@)

// (3) instance into через D73:
fn str @into() Fail[ParseIntError] -> u64 =>
    u64.from(@)

// Программист может вызвать любую из 4-х форм:
let n = u64.try_from(s)?           // → Result, propagate с ?
let n = u64.from(s)                // → throws Fail (caller handles)
let n: u64 = s.try_into()?         // → instance Result
let n: u64 = s.into()              // → instance throws
let n = u64.try_from(s).unwrap_or(0)  // → fallback default
```

**Когда писать `from` вместо `try_from`:**
- Конверсия математически не может failure'ить: numeric upcast
  (`f64.from(int)`), unit ↔ unit (`Fahrenheit.from(Celsius)`),
  newtype unwrap (`int.from(UserId)`).
- Программист может сам убедиться что параметр валиден prerequisite'ом
  (например `from(s str)` где `s` уже валидирован выше) — но это
  опасно, лучше fallible форма.

**Тонкости:**
1. **Если программист пишет ОБЕ формы** (`from` без Fail и `try_from`
   с `Result[T, !]`) — compile-error: ambiguity, какая основная.
   Программист выбирает одну.
2. **Compiler не синтезирует try-формы из infallible `from()`** —
   нет error-type для Result. Если нужно (например, generic-bound
   требует `TryFrom`), программист пишет explicit
   `T.try_from(v) -> Result[T, Never]` (Never = uninhabited error).
3. **`Result[T, Never]`** automatically converts to `T` через unwrap
   — Never-type не имеет значений, `Err` ветка unreachable.

**Когда писать `Fail`, когда нет:**
- `Fahrenheit.from(c Celsius)` — без Fail (всегда успех).
- `int.from(s str) Fail[ParseIntError]` — с Fail (может не парситься).
- `Buffer.into() Fail[Utf8Error] -> str` — с Fail (валидация UTF-8).

Это **унифицирует** API: одна форма `from`/`into` для всех конверсий.
Не нужно решать «infallible или try_»; effect-аннотация в сигнатуре
сама описывает контракт. Согласовано с D2/D10/D25/D62/D65 («всё —
эффект», throw — операция Fail).

#### Соотношение с `as` (D54)

**`as` — compile-time, без runtime-кода:**

```nova
let n = 100 as u32                 // numeric cast
let u = 42 as UserId                // newtype ↔ underlying
let code = NotFound as int          // sum → int
```

**`From` — нетривиальная конверсия с runtime-логикой:**

```nova
let f = Fahrenheit.from(c)         // арифметика
let u = User.from(json_value)      // парсинг
let m = Money.from(("USD", 100))    // конструирование с валидацией
```

Граница чёткая: если конверсия выражается одним bit-level/tag-уровнем —
`as`. Если требует логики или может бросить — `from`.

#### Соотношение с D55 record-coercion

D55 — automatic coercion в позиции с известным целевым типом для
**record-литералов** и **sum-конструкторов**:

```nova
let u User = { id: 2, name: "Bob" }     // D55: anonymous record → User
let m Maybe[int] = 42                    // D55: 42 → Just(42)
```

D73 — **explicit** конверсия через method call для произвольных типов.
D55 срабатывает раньше на синтаксическом уровне; `From.from` — обычный
вызов. Не конфликтуют:

```nova
let f Fahrenheit = Celsius(100.0)        // ОШИБКА: D55 не работает —
                                          // Fahrenheit не sum с unary Celsius
let f = Fahrenheit.from(Celsius(100.0))  // ok: D73
let f = into[Fahrenheit](Celsius(100.0)) // ok: через free function
```

#### `Into[T]` protocol и автоматический вывод

`Into[T]` — protocol с instance-методом, симметричный к `From[T]`:

```nova
type From[T] protocol {
    from(v T) -> Self          // static — на целевом типе
}

type Into[T] protocol {
    @into() -> T                // instance — на источнике
}
```

**Компилятор знает про симметрию `From`/`Into` и выводит одно из
другого автоматически.** Программист пишет **одну** реализацию из
пары, вторая выводится без блан­ket-impl и orphan-rule:

```nova
// Программист пишет From — Into выводится автоматически.
type Celsius f64
type Fahrenheit f64

fn Fahrenheit.from(c Celsius) -> Self =>
    Self((c as f64) * 9.0 / 5.0 + 32.0)

// Компилятор автоматически синтезирует:
//   fn Celsius @into() -> Fahrenheit => Fahrenheit.from(@)
// → Celsius структурно удовлетворяет Into[Fahrenheit].

let f1 = Fahrenheit.from(Celsius(100.0))    // явная from-форма
let f2 = Celsius(100.0).into()              // авто-выведенная into-форма
let f3 = into[Fahrenheit](Celsius(100.0))   // free function
let f4 Fahrenheit = into(Celsius(100.0))    // через context (D55)
```

Симметрично, если программист пишет `@into`, компилятор синтезирует
`from`:

```nova
// Программист пишет Into — From выводится автоматически.
type Json record { ... }
type User { id u64, name str }

fn Json @into() -> User =>
    User { id: @get_u64("id"), name: @get_str("name") }

// Компилятор автоматически синтезирует:
//   fn User.from(v Json) -> Self => v.into()
// → User структурно удовлетворяет From[Json].

let u1 = json.into()                        // явная into-форма
let u2 = User.from(json)                     // авто-выведенная from-форма
```

**Если написаны обе** — обе используются как написаны, авто-вывод
не применяется. **Несовпадение результатов** между руками
написанными `from` и `into` — ответственность программиста (типичный
лит-чек предупреждает, но не запрещает: бывают legitimate случаи
типа explicit-from-bytes vs implicit-into-bytes).

**Запрет циклов авто-вывода.** Авто-вывод одноуровневый: из `From[X]`
для `T` синтезируется `Into[T]` для `X`. Не наоборот в той же
итерации (это создало бы цикл). Это значит:

- Программист пишет `From[X]` или `Into[X]` — оба триггерят авто-вывод парного.
- Компилятор не пытается «найти transitively From[Y] через From[X] и From[X→Y]».

Если нужна транзитивность (`A → B → C` через две промежуточные
конверсии) — программист пишет explicit:

```nova
fn C.from(a A) -> Self =>
    let b = B.from(a)
    Self.from(b)
```

#### Две формы вызова

Конверсия доступна в **двух формах**, обе из одной реализации:

```nova
Fahrenheit.from(Celsius(100.0))       // 1. static method (From[T] protocol)
Celsius(100.0).into()                // 2. instance method (Into[T] protocol)
```

Обе формы эквивалентны. Выбирай по читаемости:

- **`T.from(v)`** — целевой тип выделен в начале, читается как
  «build a Fahrenheit from this Celsius». Хорош в выражениях,
  где тип цели — главная информация.
- **`v.into()`** — короче в method-chains: `c.into().log()`.
  Тип цели берётся из контекста (`let s str = v.into()`,
  параметр функции, return-type). Без context — компилятор
  попросит указать тип цели через аннотацию.

Free function `into[T, U From[T]](v T) -> U` **не вводится** —
третья форма создавала бы лишний выбор для программиста и LLM
(нарушение D9 «один очевидный путь»). Static `T.from` уже
покрывает explicit-type case, instance `.into()` — context-driven.

#### Throwing-варианты

`From.from` может throw'ить через `Fail[E]`:

```nova
type ParseError | InvalidFormat | OutOfRange

fn UserId.from(s str) Fail[ParseError] -> Self =>
    match parse_int(s) {
        Some(n) if n >= 0 => Self(n as u64)
        Some(_)            => throw OutOfRange
        None               => throw InvalidFormat
    }

let id UserId = UserId.from("42")        // throws Fail[ParseError]
```

Это обычная сигнатура с эффектом, никаких специальных правил.
`?` после такого вызова — нарушение D67 (`from` возвращает T через
Fail, не Result/Option):

```nova
let id = UserId.from(s)?       // ОШИБКА D67
let id = UserId.from(s)         // ok, throw сам пробрасывается
```

### Почему

1. **Нетривиальные конверсии — частая нужда.** Единицы измерения
   (`Celsius` ↔ `Fahrenheit`), парсинг (`str` → `UserId`), формат-обмен
   (`Json` → `User`). Без `From` каждый тип придумывает своё имя
   (`Celsius.to_fahrenheit`, `User.parse_json`). Единый protocol даёт
   общий контракт.

2. **Согласовано с `ToStr` (D70).** D70 уже использует ту же форму:
   protocol с одним методом + free function в prelude (`to_str(v)`).
   D73 повторяет паттерн для конверсий: `From` + `into`.

3. **`Self` универсален (D66).** `Self` в protocol-методе делает
   объявление коротким — не нужно повторять имя типа. До D66 `From[T]`
   потребовал бы typeclass-механизм; с D66 это обычный protocol.

4. **Bounds (D72) разблокируют generic-функции.** `fn parse[U From[str]]`
   до D72 было невозможно. Теперь — естественно.

5. **Прецедент Rust.** `From`/`Into` — самый используемый паттерн в
   Rust ecosystem. Nova берёт идею (явные конверсии через protocol),
   адаптирует под свою систему (структурная типизация, без orphan
   rule, free function вместо blanket-impl).

6. **AI-friendly.** LLM генерирует `Fahrenheit.from(celsius)` без
   обдумывания имени метода. Структурный bound `[U From[T]]`
   проверяется compile-time с понятной ошибкой («`Bar` не реализует
   `From[Foo]`: missing static method `from(v Foo)`»).

### Что отвергнуто

- **Free function `into[T, U From[T]](v T) -> U`.** Раньше была
  предложена как третья форма вызова (`into[Target](value)`).
  Отвергнута: дублирует `T.from(v)` (ровно та же ширина и информация),
  создаёт три формы для одной операции — нарушение D9. `T.from`
  для explicit-type, `v.into()` для context-driven — этих двух
  достаточно.
- **Только `From[T]` без `Into[T]`** (как было в первой редакции D73).
  Без `Into` method-form `c.into()` была недоступна. Теперь
  `Into[T]` — first-class protocol; method-form работает; компилятор
  выводит парность из `From[T]` автоматически.
- **Blanket-impl типа Rust `T: From<U> ⇒ U: Into<T>`.** В Nova нет
  orphan rule и нет `impl` блоков (D42/D53), классический blanket-impl
  негде. **Решение Nova** — компилятор синтезирует парный protocol
  на уровне type-checker'а: если у типа есть `from`, считается что
  есть и `@into` (и наоборот). Это сохраняет преимущество Rust
  (одна реализация → две формы вызова) без orphan-механики.
- **`From` как trait с default-методами.** Без `impl` блоков и orphan
  rule концептуально неприменимо. Авто-синтез symmetric'а заменяет.
- **Implicit conversion в позиции аргумента** (Scala 3 `Conversion`,
  C++ implicit constructors). Nova: все конверсии явные (`as`, `from`,
  D55 — но D55 only для sum/record-литералов, без method call).
- **`@from(v T) -> Self` instance-метод вместо static.** `from` это
  фабрика — у неё нет существующего инстанса для `@`. По D35
  `fn Type.method` для конструкторов / static, что соответствует
  семантике.
- **`as` для нетривиальных конверсий** (`celsius as Fahrenheit`).
  D54 явно ограничивает `as` — compile-time numeric/newtype/sum.
  Расширять — теряется граница между cheap-cast и expensive-conversion.
- **Отдельный `ToStr` protocol для конверсии в строку (старая D70).**
  Конверсия в `str` — частный случай `From[X]`-механизма. Иметь два
  механизма для одной задачи нарушает D9. См. D70 v3 «REPLACED → D73»
  про переход.

### Цена

1. **Без context требуется явный целевой тип.** `v.into()` на
   bare-line-position не компилируется — нужно либо `let x T = v.into()`,
   либо `T.from(v)` с явным типом-prefix'ом.
2. **Multiple `From[X]` через overloading по типу параметра** (D46) —
   зависит от Q-overloading-rules. В MVP overloading по типу аргумента
   разрешён в D46, но детали ambiguity ещё не финализированы.
3. **`From` от типа из чужого модуля.** Без orphan rule — добавляешь
   `fn MyType.from(v ForeignType)` где угодно, **но** реализация
   живёт в модуле, владеющем `MyType` (по D47 visibility). Если ни
   один из типов не «твой» — добавить `From` нельзя без обёртки
   (newtype). Это сознательное ограничение: предотвращает duplicate
   conflicting implementations.

### Связь

- [02-types.md → D53](02-types.md#d53) — protocol = тип, основа.
- [02-types.md → D66](02-types.md#d66) — `Self` в protocol.
- [02-types.md → D72](02-types.md#d72) — bounds для `[U From[T]]`.
- [03-syntax.md → D35](03-syntax.md#d35) — static / instance методы;
  receiver — любой тип, включая примитивы (`fn str.from(...)`).
- [03-syntax.md → D54](03-syntax.md#d54) — `as` для тривиальных
  cast'ов; D73 покрывает остальное.
- [02-types.md → D55](02-types.md#d55) — record/sum coercion;
  D73 для остальных типов.
- [04-effects.md → D67](04-effects.md#d67) — `from` с throw через
  `Fail` следует общим правилам `?`.
- [08-runtime.md → D70](#d70-tostr-protocol--to_str-метод--free-function-tostrv)
  — REPLACED → D73; конверсия в `str` это частный случай D73.
- [D26](#d26-базовая-stdlib-и-prelude) — `From`, `Into` в prelude.

### Открытые вопросы

- **`From` для базовых типов.** Stdlib pre-registers `str.from(int)`,
  `str.from(bool)`, `str.from(f64)` (D70-replacement). Должны ли
  `int.from(bool)`, `f64.from(int)` etc. — сейчас open вопрос
  Q-from-builtins.
- **`TryFrom`** — отдельный protocol для **fallible** конверсий
  с явным `Result`/`Fail` в сигнатуре? Сейчас обычный `from` с
  `Fail[E]` достаточен. Q-tryfrom.
- **Auto-derive `From`** — для newtype можно автоматически (`type
  UserId u64` ⇒ `UserId.from(n u64) -> Self`)? Сейчас программист
  пишет вручную. Q-auto-from.
- **`From`-цепочки.** Если `B: From[A]` и `C: From[B]`, можно ли
  одно вызовом перейти `A → C`? В Rust — нет (single-step). Nova —
  пока тоже нет, программист пишет `C.from(B.from(a))`. Q-from-chain.

### Эволюция

**v1 (первая редакция D73):** только `From[T]` protocol + free function
`into[T, U From[T]](v T) -> U`. `Into` отвергнут как «Rust-style
blanket-impl нет, не нужен отдельный protocol». Method-form
`value.into()` не работала.

**v2:** добавлен `Into[T]` protocol с instance-методом `@into() -> T`.
Компилятор автоматически синтезирует парный protocol — `T.from(v X)`
written → `X.into() -> T` synthesized (и наоборот). Три эквивалентные
формы вызова из одной реализации: `into[T](v)`, `v.into()`,
`T.from(v)`.

**v3 (текущая, 2026-05-06):** убрана free function `into[T, U](v)`.
Три формы — это нарушение D9. Остались две: `T.from(v)` (static,
explicit-type) и `v.into()` (instance, context-driven). Также:

- D70 `ToStr` помечен как REPLACED → D73 — конверсия в строку
  выражается через `str.from(v)` / `v.into()` (с context = str).
- D35 явно расширен: receiver-тип может быть примитивом
  (`fn str.from(int)`, `fn int @to_hex() -> str` и т.п.).

**Что было невозможно до этого:** D73 как механизм требует bound'ы
(D72). До D72 (Q-bounds открыт) `From`/`Into` пара была заблокирована.
С D72 разблокирована.

---

## D74. Математические операции на числовых типах — instance-методы

### Что
Стандартные математические функции (`sin`, `cos`, `sqrt`, `atan2`,
`hypot`, `abs`, `pow`, `floor`, `is_finite`, и др.) объявляются как
**instance-методы** через `@` на числовых типах (`f64`, `f32`, `int`,
i8-i64, u8-u64), а не как static `Math.fn(...)` или free function
`sin(x)`. Static-функции остаются только для **констант**
(`f64.PI`, `f64.NAN`) и **парсинга** (`f64.try_parse(s)`).

```nova
let r = (x * x + y * y).sqrt()
let phi = im.atan2(re)
let dist = a.hypot(b)
let s = (theta + offset).sin()
let n = magnitude.abs()
```

### Правило

#### Полный набор на `f64` (prelude)

| Категория | Методы |
|---|---|
| Корни и степени | `@sqrt()`, `@cbrt()`, `@sqr()`, `@pow(exp f64)`, `@powi(n int)` |
| Тригонометрия | `@sin()`, `@cos()`, `@tan()`, `@asin()`, `@acos()`, `@atan()` |
| `atan2` (двух-арг) | `@atan2(other f64) -> f64` (`y.atan2(x)`) |
| Гиперболические | `@sinh()`, `@cosh()`, `@tanh()` |
| Экспонента / лог | `@exp()`, `@ln()`, `@log10()`, `@log2()`, `@log(base f64)` |
| Норма / расстояние | `@abs()`, `@hypot(other f64)` |
| Округление | `@floor()`, `@ceil()`, `@round()`, `@trunc()`, `@fract()` |
| Знак / минимум | `@signum()`, `@min(other f64)`, `@max(other f64)` |
| Предикаты | `@is_finite()`, `@is_nan()`, `@is_infinite()` |

Аналогичный набор на `int` (где математически осмысленно):
`@abs()`, `@pow(n int)`, `@signum()`, `@min(other)`, `@max(other)`,
`@is_negative()`, `@is_positive()`. Тригонометрия и логарифмы — только
на float-типах.

#### Static-функции на типе (не методы)

Для констант и операций без естественного receiver'а — обычные
static через точку (D35):

```nova
f64.PI                                    // константа π
f64.E                                     // константа e
f64.NAN                                   // тихий NaN
f64.INFINITY                              // +∞
f64.NEG_INFINITY                          // -∞
f64.MAX                                   // максимальное конечное
f64.MIN_POSITIVE                          // минимальное положительное
f64.EPSILON                               // машинная точность

f64.try_parse(s str) -> Option[f64]      // парсинг с возможной ошибкой
```

Парсинг через `f64.try_parse(s)` дополнен `From[str]` через D73 —
доступна обе формы:

```nova
let x = f64.try_parse("3.14")            // Option[f64]
let y f64 = f64.from("3.14")              // throws Fail[ParseError]
let z f64 = "2.71".into()                 // через D73 авто-Into
```

#### Двух-аргументные функции

`atan2`, `hypot`, `min`, `max`, `pow`, `log` принимают два аргумента.
Receiver — первый по математической / физической конвенции:

```nova
y.atan2(x)        // arctangent of y/x — y первый
a.hypot(b)        // √(a² + b²) — симметрично, но a первый
base.log(other)   // log_base(other)
x.pow(n)          // x^n
```

Это даёт chain-style: `dy.atan2(dx).abs() < tolerance`.

#### Соответствующее имя `@sqr()`

`@sqr()` — квадрат (`x*x`). Имя из Pascal (`Sqr(x)`), короче
`squared`, согласовано с одноимённым методом на других типах
(например, `Complex @sqr()`). Для нецелых степеней — `@pow(2.0)`
или `@powi(2)`.

### Почему

1. **Согласовано с D35** ([03-syntax.md → D35](03-syntax.md#d35)).
   `@`-методы — основной механизм для type-bound функций. Числовые
   операции — type-bound по определению (зависят от типа: `i32.abs()`
   ≠ `f64.abs()` в реализации). Использовать static-стиль для одних
   операций и `@` для других — нарушение D40 «один способ».

2. **Chain-friendly формулы.** Длинные математические выражения
   читаются слева направо в «pipeline»-стиле:
   ```nova
   let result = (a*a + b*b).sqrt().abs().min(MAX_VALUE)
   ```
   В static-стиле было бы:
   ```nova
   let result = f64.min(f64.abs(f64.sqrt(a*a + b*b)), MAX_VALUE)
   ```
   Вложенность растёт справа налево, читать тяжелее.

3. **Прецедент Rust / Kotlin / Swift.** Все три используют instance-
   методы для математики (`(2.0_f64).sqrt()`, `theta.cos()`).
   Java/JS/Python со static-стилем (`Math.sin(x)`) — наследие старой
   эпохи без object-методов на примитивах.

4. **Free functions конфликтуют с user-кодом.** `sin(x)` как глобальная
   функция занимает имя `sin` — пользователь не может назвать так
   свою функцию без shadowing prelude. `@sin()` живёт в namespace
   типа, не глобально.

5. **AI-friendly.** LLM пишет `theta.cos()` без раздумий «math.cos
   или Math.cos или просто cos». Один паттерн — один способ
   вызова.

### Что отвергнуто

- **Static `Math.sin(x)`** (Java, JavaScript). Менее читаемо для
  длинных формул, не chain-friendly, и в Nova нет объекта-namespace
  `Math` (нет static-namespace объектов как в Java).
- **Free function `sin(x)`** (C, Python). Захватывает короткие имена
  в глобальном scope, конфликтует с пользовательскими функциями.
- **Trait-style `Float` protocol с `sin/cos/...`** (Haskell `Floating`,
  Rust `num_traits::Float`). Лишняя indirection, generics с bounds
  для каждой математической функции усложняют сигнатуры. В Nova
  `f64`/`f32` — отдельные типы, дублирование методов на оба
  допустимо (как в Rust).
- **Разные имена для разных размеров** (`sinf` для f32, `sin` для f64
  как в C). Перегрузка по типу receiver'а (D35) даёт одно имя, разные
  реализации — естественно для языка с типами.
- **`@squared()` вместо `@sqr()`.** Длиннее без выгоды; `sqr` имеет
  Pascal-прецедент и согласовано со стилем коротких имён в Nova
  (`@neg`, `@inv`, `@conj`, `@arg`, `@rem`, `@shl`).
- **Только static-функции для констант + instance для операций
  через `@`** (mixed). Принято: константы — static (`f64.PI` — у
  значения нет receiver'а), операции — `@`. Это два разных рода
  имён (decleration site), не конфликт.

### Цена

1. **Дублирование методов между f32/f64**, потенциально int.
   Реализация — обычно одна (через builtin / FFI к libm), но
   объявления повторяются. Это цена отсутствия Float-protocol;
   терпимо для prelude, который пишется один раз.

2. **`x.sqrt()` для `x < 0`** возвращает `NaN` (IEEE 754) — runtime-
   surprise. Strict-режим (`Fail[NaN]`) — отдельная функция
   `@try_sqrt()` если понадобится; в base — IEEE без проверок.

3. **Нет namespace `math`.** Если пользователь хочет
   `import math; math.sin(x)` — придётся писать `x.sin()`. Часть
   программистов из Python/Java будут удивлены поначалу.

### Связь

- [D26](#d26-базовая-stdlib-и-prelude) — prelude содержит математику
  как часть числовых типов; D74 уточняет форму объявления.
- [03-syntax.md → D35](03-syntax.md#d35) — `@`-методы как механизм.
- [03-syntax.md → D46](03-syntax.md#d46) — operator overloading
  (`@plus`, `@times`, ...) дополняет D74 для арифметики.
- [03-syntax.md → D40](03-syntax.md#d40) — «один способ» — выбор
  между static и instance не остаётся на усмотрение программиста.
- [D73](#d73-from--into-protocol-пара-с-авто-выводом) — парсинг
  чисел через `f64.from(s)` / `s.into()`, согласовано с from/into.
- [std/math/complex.nv](../../std/math/complex.nv) —
  использует instance-стиль (`theta.cos()`, `im.atan2(re)`,
  `a.hypot(b)`) как канонический пример.

### Эволюция

Изначально черновик `complex.nv` (2026-05) использовал static-стиль
`f64.cos(theta)`, `f64.atan2(im, re)` по аналогии с Java `Math.sin`.
При обсуждении выявлено что это противоречит D35 (методы — основной
механизм) и плохо читается для математических формул. Все вызовы
переписаны в instance-стиль, и паттерн зафиксирован формальным
D-решением D74.

`Math` namespace отвергнут (нет static-namespace в Nova, имя `Math`
конфликтовало бы с пользовательскими типами `Math` для предметных
областей).

---

## D77. `TryFrom` / `TryInto` — protocol-пара, расширение D73 для fallible-конверсий

> **Уточнение (2026-05-07):** D73 теперь сам поддерживает fallible
> через `Fail[E]` в сигнатуре `from`/`into` — единый механизм.
> Программист пишет **одну** из 4-х форм (`from` / `into` / `try_from` /
> `try_into`), компилятор синтезирует остальные. **Рекомендуется
> писать `try_from`** для fallible (Result-стиль явный, error type
> first-class в signature) и `from` для infallible (без boilerplate
> `Ok(...)`). Подробности в D73 «Auto-derive 4-way».
>
> Этот документ (D77) описывает Result-форму (`try_from` / `try_into`)
> как **рекомендуемую implementation form** для fallible конверсий
> (вопреки названию «convenience sugar» в раннем описании).

### Что
Парный механизм к [D73](#d73-from--into-protocol-пара-с-авто-выводом)
для **fallible-конверсий**: когда конверсия может не получиться,
программист может выбрать одну из двух эквивалентных форм:

1. **Throwing-форма** через `Fail[E]` — `T.from(v) Fail[E] -> Self`
   (D73, основная форма).
2. **Result-форма** — `T.try_from(v) -> Result[Self, E]` (D77,
   convenience sugar).

Семантически **эквивалентны** (одна задача — конверсия с возможной
ошибкой), различаются **формой возврата ошибки**. D73 forma — Nova-
канонический путь («всё — эффект», D2/D10), D77 — для error-aware
веток с explicit Result.

**Компилятор синтезирует одну из другой.** Программист пишет одну
сторону, другая выводится — точно так же как `From` ↔ `Into` в D73.

```nova
// Программист пишет — одну форму:
fn u64.try_from(s str) -> Result[Self, ParseIntError] => ...

// Компилятор автоматически даёт обе формы вызова:
let n = u64.from("42")             // throws Fail[ParseIntError]
let r = u64.try_from("42")          // Result[u64, ParseIntError]
let opt = u64.try_from("42").ok()   // Option[u64] через Result.ok()
```

`Option`-вариант **не** требует отдельного метода — `Result.ok()`
из prelude превращает Result в Option. Один универсальный путь.

### Правило

#### Декларация protocol'ов в prelude

```nova
type TryFrom[T, E] protocol {
    try_from(v T) -> Result[Self, E]
}

type TryInto[T, E] protocol {
    @try_into() -> Result[T, E]
}
```

`Self` (D66) — реализующий тип. `try_from` — static-метод (как
обычный `from`), `try_into` — instance-метод.

#### Авто-синтез четырёхугольника

Если программист пишет любую **одну** форму из четырёх, компилятор
выводит остальные три:

```nova
       T.from(v X)              ← throws Fail[E]
       T.try_from(v X)          ← Result[Self, E]
       v.into() -> T            ← throws Fail[E]
       v.try_into() -> T        ← Result[T, E]
```

**Правила синтеза:**

1. **`from` → `try_from`:** оборачивает throw в Result.
   ```nova
   // Если написано:
   fn u64.from(s str) Fail[ParseIntError] -> Self => ...
   // Синтезируется:
   fn u64.try_from(s str) -> Result[Self, ParseIntError] =>
       with Fail[ParseIntError] = (e) => interrupt Err(e) {
           Ok(Self.from(s))
       }
   ```

2. **`try_from` → `from`:** разворачивает Result в throw.
   ```nova
   // Если написано:
   fn u64.try_from(s str) -> Result[Self, ParseIntError] => ...
   // Синтезируется:
   fn u64.from(s str) Fail[ParseIntError] -> Self =>
       match Self.try_from(s) {
           Ok(v)  => v
           Err(e) => throw e
       }
   ```

3. **`from` ↔ `into` / `try_from` ↔ `try_into`:** через D73-механизм
   на каждой из форм отдельно. То есть если написано `u64.from(s)`,
   синтезируются:
   - `u64.try_from(s)` (D77)
   - `s.into()` для типа `u64` (D73)
   - `s.try_into()` для типа `u64` (D77)

**Если написаны обе** (например, `from` и `try_from` обе вручную) —
обе используются как написаны, авто-синтез не применяется. Как в D73,
программист отвечает за consistency.

#### Какую форму писать?

Рекомендация — **писать `try_from`**, для парсинга / валидации:

```nova
fn u64.try_from(s str) -> Result[Self, ParseIntError] =>
    if !is_all_digits(s) {
        Err(InvalidDigit { position: 0 })
    } else {
        // ... основная логика
        Ok(parsed_value)
    }
```

Причины:
- **Result-возврат явный** — программисту не нужно держать в голове
  активный handler `Fail[E]`.
- **Тип ошибки виден в сигнатуре** (`Result[Self, ParseIntError]`),
  а не пробрасывается через эффект-row (где может теряться).
- **Pattern matching** на Result удобен внутри парсера для composition.

`from` остаётся для случаев когда программист **уверен** в успехе и
не хочет писать `match`:

```nova
fn UserId.from(n u64) -> Self => Self(n)         // infallible
fn Greeting.from(name str) -> Self =>
    Self("Hello, ${name}!")                       // тоже infallible
```

Если конверсия **infallible** — `from` достаточно, `try_from` не
синтезируется (нет `E`).

#### Семантика равенства

`from(s)` и `try_from(s).unwrap()` — поведенческое равенство (с
учётом разной формы ошибки). Компилятор гарантирует:
- `try_from(v) == Ok(x)` ⇒ `from(v) == x`
- `try_from(v) == Err(e)` ⇒ `from(v)` бросает `throw e`

#### `D67` ?-оператор

- `let v = u64.try_from(s)?` — **валидно**, Result оборачивается
  через [D67](04-effects.md#d67) `?` на Result.
- `let v = u64.from(s)?` — **ошибка** (D67), `from` возвращает T
  через `Fail`, не Result. Throw сам пробрасывается без `?`.

```nova
// Функция возвращает Fail[ParseIntError]:
fn parse_pair(s str) Fail[ParseIntError] -> (u64, u64) {
    let parts = s.split(",")
    let a = u64.from(parts[0])              // throws через Fail (без ?)
    let b = u64.from(parts[1])              // throws через Fail (без ?)
    (a, b)
}

// Функция возвращает Result, использует try_from + ?:
fn parse_pair_r(s str) -> Result[(u64, u64), ParseIntError] {
    let parts = s.split(",")
    let a = u64.try_from(parts[0])?         // ? на Result (D67)
    let b = u64.try_from(parts[1])?
    Ok((a, b))
}
```

#### Option через `Result.ok()`

Отдельный `try_parse` / `from_str_or_null` / similar **не вводится**.
Если нужен Option — `Result.ok()` в prelude:

```nova
fn Result[T, E] @ok() -> Option[T] => match @ {
    Ok(v)  => Some(v)
    Err(_) => None
}

// Использование:
let opt = u64.try_from(s).ok()          // Option[u64]
match u64.try_from(s).ok() {
    Some(n) => n
    None    => default_value
}
```

Прецедент Rust: `s.parse::<u64>().ok()` → `Option<u64>`. Один
универсальный путь, не требует отдельного именования.

### Почему

1. **Согласовано с D73.** Тот же auto-pair-механизм. Программист
   видит ровно один паттерн «пишу одну сторону — компилятор даёт
   все формы вызова». Не нужно помнить «for fallible — другая система».

2. **Закрывает три формы вызова через одну реализацию.** Парсинг —
   частый use case. Без D77 программисту нужно либо:
   - Писать `try_X` отдельно (Kotlin-style `toIntOrNull`, размножение
     имён), или
   - Всегда `match { Some => ... None => throw }` обёртку.

3. **Стандартизованное имя `try_from`.** До D77 разные библиотеки
   могли использовать `try_parse`, `parse_or_err`, `validate`, и
   т.д. — каждая со своим именем. С D77 — единое имя как `from`
   стандартно для конверсии.

4. **Прецедент Rust:** `From` / `TryFrom` — стандарт `std`. Auto-blanket
   реализация (`Into ↔ From`) делается компилятором. Nova повторяет
   паттерн.

5. **Option получается бесплатно** через `Result.ok()`. Не нужны
   `_or_null`-suffix имена (Kotlin), `init?` (Swift), `*OrNull`
   (Java fluent helpers). Один Result — три формы (`from`, `try_from`,
   `try_from(...).ok()`).

6. **AI-friendly.** LLM пишет `Version.from(s)` и работает; пишет
   `Version.try_from(s)?` для propagation через Result — тоже
   работает. Не нужно помнить какая форма реализована — всегда обе
   доступны.

### Что отвергнуто

- **`u64.try_parse(s) -> Option[u64]`** — отдельный Option-вариант
  как метод. Конфликтует с принципом «один способ» (D9): `try_parse`
  vs `try_from(...).ok()` делают одно и то же. Result.ok() универсальнее.
- **`u64.parse(s)`** — отдельное имя для парсинга. Парсинг — это
  частный случай конверсии (`str → u64`), общий механизм через
  `from`/`try_from` лучше.
- **`OrNull`-suffix имена** (Kotlin): `toIntOrNull`. Размножение
  имён, не масштабируется (`fromOrNull`, `intoOrNull`, `parseOrNull`).
- **Java-style overloading throwing/non-throwing с одинаковым именем**
  (`int.parse(s) -> int` vs `int.parse(s) -> int` через флаг).
  Тип-ambiguity, нечитаемо.
- **Failable initializer как в Swift** (`init?`). Специальный
  синтаксис конструктора — лишняя категория. У Nova `from`/`try_from`
  обычные функции.

### Цена

1. **Расширение compiler-логики.** D73 уже синтезирует пару From/Into,
   D77 удваивает: from/try_from + into/try_into = 4 формы из одной
   написанной. Компилятор должен:
   - Распознать одну из четырёх форм
   - Сгенерировать остальные три
   - Применять одни и те же правила structural-conformance.
   Цена — реализация в type-checker'е, не run-time.

2. **Semantic equivalence требует доверия.** Компилятор гарантирует
   что `from(v)` и `try_from(v).unwrap()` поведенчески одинаковы.
   Если программист пишет **обе вручную** и они расходятся —
   ответственность программиста (как в D73).

3. **Ambiguity при нескольких `try_from`.** Если у `u64` есть
   `try_from(str)` и `try_from(f64)` (через overloading D46) —
   `u64.try_from(x)` резолвится по типу аргумента. Стандартный D46
   overloading.

4. **`Self` в Result.** `Result[Self, E]` корректно по D66 (Self
   валиден в method-контексте). Generic-параметр `E` свободен —
   не привязан к Self.

### Связь

- [D73](#d73-from--into-protocol-пара-с-авто-выводом) — базовая
  пара From/Into, D77 расширяет на fallible-форму.
- [D67](04-effects.md#d67) — `?`-оператор; работает на Result
  (`try_from(s)?`), не работает на throwing `from`.
- [D72](02-types.md#d72) — bounds: `[U TryFrom[T, E]]` для
  generic-функций fallible-конверсии.
- [D26](#d26-базовая-stdlib-и-prelude) — `TryFrom`, `TryInto`,
  `Result`, `Option` в prelude. `Result.ok() -> Option[T]` — стандартный
  метод для перевода.
- [D30](03-syntax.md#d30) — конвенция имён ошибок
  (`Parse<TypeName>Error`); не меняется.
- [std/data/semver.nv](../../std/data/semver.nv) —
  использует `u64.try_parse` (legacy имя) — должно мигрировать на
  `u64.try_from` после принятия D77.

### Открытые вопросы

- **Auto-derive для newtype?** `type UserId u64` — должны ли
  автоматически быть `UserId.from(n u64)` и `UserId.try_from(s str)`?
  Сейчас — программист пишет вручную. Q-auto-from осталось открытым
  из D73, расширяется на D77.
- **`from` цепочки** (`A → B → C`) — ни D73, ни D77 не вводят
  транзитивность. Программист пишет `C.from(B.from(a))`. Q-from-chain.
- **`TryFrom` для одного и того же `T` с разными `E`?** Пример:
  `u64.try_from(s str) -> Result[Self, ParseIntError]` и
  `u64.try_from(s str) -> Result[Self, ValidateError]` — отличаются
  только `E`. По D46 overloading методов по типу аргумента работает,
  но здесь типы аргументов одинаковы (`str`), а `E` это return-type
  параметр — overloading по return-type Nova не поддерживает.
  Решение: использовать `enum`-объединение ошибок (`type AnyError | A | B`)
  или разные имена. Q-tryfrom-multi-error.

### Эволюция

До D77 в первой реализации `std/data/semver.nv` использовался
`u64.try_parse(s) -> Option[u64]` — отдельное имя для Option-варианта
парсинга. При обсуждении выявилось три проблемы:

1. **Ad-hoc имя** — каждая stdlib-либа могла использовать своё
   (`try_parse`, `parse_opt`, `from_str_or_null`).
2. **Дублирование с `from`** — `try_parse` это «`from` минус throw,
   плюс Option». Семантически избыточно.
3. **Прецедент Rust** — `TryFrom` парный к `From` решает ту же
   задачу унифицированно.

D77 формализует: **одно имя `try_from`** для Result-варианта, авто-
синтез четырёх форм вызова из одной реализации. Option получается
через `Result.ok()`. `try_parse` отвергается как избыточное.

Backward-compat: `try_parse` в существующих файлах (semver.nv) —
переименовывается на `try_from`. Общая семантика не меняется.

---

## D76. `Mem` эффект — runtime introspection для leak/growth тестов

> **Status:** active. **Реализовано** в bootstrap'е (2026-05-06).
> Тесты: `nova_tests/runtime/memory_growth.nv`.

### Что

Built-in эффект `Mem` даёт Nova-коду доступ к runtime-счётчикам
аллокаций. Цель — **regression detection**: тест запоминает
`Mem.alloc_count()` до и после горячего кода и assert'ит, что прирост
остался в разумном бюджете. Если codegen начнёт генерировать в N раз
больше аллокаций (баг типа "alloc-per-iter увеличился на порядок"),
тест поймает это сразу.

### Операции

```nova
Mem.alloc_count() -> int   // total nova_alloc since gc_init/reset
Mem.free_count()  -> int   // total frees (plain malloc backend → 0)
Mem.live()        -> int   // alloc_count - free_count
Mem.reset()       -> ()    // zero stats counters (for per-test isolation)
```

Числа — это **счётчики вызовов**, не байты. Этого достаточно для
поимки регрессий "1 alloc на итерацию стало 10".

### Семантика

- `Mem` pre-registered как built-in эффект (как `Time`, `Fail`).
  Compiler не требует `Mem` в сигнатуре функции — это ambient
  capability (D11 / D62-style).
- **Нет user-handler'а:** в отличие от `Time` и `Fail`, операции
  `Mem` не имеют vtable; они эмитируются прямо в `Nova_Mem_*`
  inline-функции, которые ходят к runtime-counters.
  *Причина:* эти операции должны быть **наблюдаемыми с очень
  низкими накладными расходами** — vtable добавляет лишний indirect
  call который сам бы изменил alloc-pattern. И смысла переопределять
  их нет (это не business effect — это runtime-факт).

### Реализация

- **`compiler-codegen/nova_rt/alloc.h`** — runtime-функции
  `nova_gc_alloc_count`, `nova_gc_free_count`, `nova_gc_live_count`,
  `nova_gc_reset_stats`. Доступны во всех allocator-backend'ах.
- **`compiler-codegen/nova_rt/alloc.c`** (Phase-0 plain malloc) —
  считает `nova_alloc` calls; `free_count` всегда 0 (`release`
  no-op). Достаточно для growth-rate тестов.
- **`compiler-codegen/nova_rt/effects.h`** — `Nova_Mem_*` inline-
  обёртки.
- **`compiler-codegen/src/codegen/emit_c.rs`** — `effect_schemas`
  pre-populated с `Mem` schema; standard effect-call dispatch
  работает (`Mem.live()` → `Nova_Mem_live()`).

### Bootstrap-ограничения

1. **Plain-malloc backend (default):** `free_count` всегда 0,
   `live` == `alloc_count`. Это значит leak-тесты могут только
   измерять **growth rate**, не "осталось ли что-то живое". Когда
   подключим Boehm GC (alloc_boehm.c) или RC (alloc_rc.c) —
   free_count станет осмысленным, тесты можно расширить.
2. **Нет per-allocation type info.** `alloc_count` — счётчик всех
   `nova_alloc` calls без разбивки по типам. Production-runtime
   возможно даст breakdown (records, arrays, fiber stacks).
3. **Не thread-safe** в multi-threaded backend'е (счётчики не
   atomic). На bootstrap single-threaded fiber-runtime это OK.

### Связь

- [D7](#d7-один-язык--три-режима-компиляции) — runtime modes;
  `Mem` доступен во всех режимах.
- [D11](04-effects.md#d11) — pre-registered effects pattern.
- [05-memory.md → D6](05-memory.md#d6) — managed-heap design;
  `Mem` — observability над ним.

### Что отвергнуто

- **Free function `mem_alloc_count()`** — нарушает D9 («одна
  идиома для одной задачи»). Effect-форма даёт ровно столько же
  выразительности и согласована с Time.
- **Bytes-tracking** в bootstrap — требует instrumentированного
  allocator (overhead). Counts достаточно для regression-detection.

---

## D81. `assert(cond)` vs `debug_assert(cond)` — build-mode семантика

### Что

Два уровня assertion'ов в prelude:

- **`assert(cond)`** — **always runtime**, проверяется во всех
  режимах сборки (debug/release/JIT/AOT). Failure → panic
  ([D13](#d13)).
- **`debug_assert(cond)`** — **debug-only**, в release-сборке
  полностью отбрасывается компилятором (zero cost).

Третий уровень — формальные контракты `requires`/`ensures`
([D24](09-tooling.md#d24)) — отдельный механизм, не путать.

### Правило

#### Декларация в prelude

```nova
// always runtime — production invariants
fn assert(cond bool) -> ()

// debug-only — hot-path / sanity checks
fn debug_assert(cond bool) -> ()
```

Сигнатуры идентичны на уровне типов; разница — в семантике релиза.
Обе — обычные prelude-функции (не keyword'ы), вызываются со скобками
как любой fn-call (см. также [syntax.md секция «Тестирование без
моков»](../syntax.md)).

#### Семантика по build-mode

| Form | Compile-time check | Debug runtime | Release runtime | Use-case |
|---|---|---|---|---|
| `assert(cond)` | нет | check | **check** | production invariants |
| `debug_assert(cond)` | нет | check | **no-op** | hot-path / sanity |
| `requires`/`ensures` (D24) | SMT где возможно | check rest | **no-op** | formal contracts |

#### Примеры использования

```nova
// Production invariant — всегда проверяется
fn divide(a int, b int) -> int {
    assert(b != 0)            // ВСЕГДА runtime, даже в release
    a / b
}

// Hot-path — release не платит за проверку
fn fast_lookup(arr []int, idx int) -> int {
    debug_assert(idx >= 0 && idx < arr.len)   // только в debug
    arr[idx]                                    // unchecked в release
}

// Формальный контракт — compile-time где возможно, runtime fallback
fn sqrt(x f64) -> f64
    requires x >= 0.0
    ensures result >= 0.0
=> ...
```

#### Build-mode mechanics в bootstrap

Bootstrap (D71) **не различает** debug/release — все три режима
([D7](#d7-один-язык--три-режима-компиляции)) одинаковы, всегда
checked. `debug_assert` в bootstrap'е — **синоним `assert`** (тот же
runtime check, готовность к production-семантике).

Production-runtime добавит:
- preprocessor-style `#ifdef NOVA_DEBUG` для C-backend, или
- codegen-флаг для no-op generation в release-сборке.

Build-mode влияет на **performance**, не на **семантику** программы:
`assert` всегда работает; `debug_assert` — только performance в release.
Это согласовано с D7 принципом «один язык — три режима».

### Почему `assert` = always runtime (не Java/C-style no-op)

1. **AI-friendly: одна семантика.** LLM генерирует `assert(...)`
   ожидая, что invariant держится. Если в release он silent — это
   **тихий bug class** (Java pre-1.4 classic).

2. **Безопасность.** «Production runs without your invariants» —
   известная проблема C/Java/Python: программист в курсе своих
   asserts только в debug, в release они **исчезают** без следа.

3. **Прецедент Rust/Swift.** `assert!` в Rust always runtime;
   `debug_assert!` для debug-only. Swift аналогично: `assert`
   debug-only, `precondition` always runtime — но Nova инвертирует
   defaults (более безопасный — короткое имя).

4. **Согласовано с D24.** Если программист хочет zero-cost проверку
   с compile-time гарантией — пишет `requires` (D24 contract). Если
   просто debug-time hint — `debug_assert`. `assert` — strong
   invariant, всегда работает.

5. **D13 (panic vs effects).** `assert` failure = panic = fiber dies.
   Это «hardware/math сбой» класс, не business error. По D13 такое
   **не должно зависеть от build-mode**.

### Что отвергнуто

- **`assert` no-op в release** (C/Java/Python style). Тихие bug'и в
  production — главная причина отказа.
- **`assert` как keyword без скобок** (Rust macro / Java `assert`
  expression). Закрыто в spec sweep 2026-05-07: assert — обычная
  fn-call, со скобками. Один способ для одной задачи (D40).
- **Только один уровень (`assert` always runtime).** Hot-path
  use-case реален; без `debug_assert` программисты пишут
  `if (DEBUG) { ... }` ручками. Лучше дать canonical-форму.
- **Только один уровень (`assert` debug-only).** Невозможно выразить
  production invariant. Java pre-1.4 опыт показывает что это
  anti-pattern.

### Связь

- [D7](#d7-один-язык--три-режима-компиляции) — три режима компиляции;
  D81 уточняет, как build-mode влияет на assert-семантику.
- [D13](#d13-panic-vs-эффекты-что-не-является-эффектом) — assert
  failure = panic, не Fail-эффект.
- [D24](09-tooling.md#d24) — `requires`/`ensures` контракты;
  D81 определяет три уровня safety: `assert` < `debug_assert` <
  `contracts`.
- [D26](#d26) — prelude содержит обе функции (`assert`,
  `debug_assert`).
- spec/syntax.md — секция «Тестирование без моков» уточняет, что
  `assert(cond)` обязательно со скобками (fn-call).

### Эволюция

До 2026-05-07 spec упоминал `assert` неявно — в `syntax.md` как
«встроенный оператор» (без скобок), в D26 prelude как функцию (со
скобками). Bootstrap-парсер принимал только со скобками.
spec-assert-syntax sweep 2026-05-07 канонизировал форму
`assert(cond)` — функция из prelude, обязательно со скобками.

D81 закрывает оставшийся вопрос — **семантика в release**.
Принята модель Rust (`assert!` always runtime + `debug_assert!`
debug-only). До D81 spec не различал `assert`/`debug_assert`,
bootstrap имел только always-runtime `nova_assert` без build-mode
разделения. После D81: prelude содержит обе функции; production-
runtime реализует zero-cost `debug_assert` в release; bootstrap
оставляет `debug_assert` как alias `assert` до production.
