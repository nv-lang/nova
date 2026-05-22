# Контракты и формальная верификация в Nova

[English](contracts.md) | **Русский**

Система контрактов Nova позволяет описать, что функция **требует** и
**гарантирует**, и проверяет эти утверждения на этапе компиляции через
SMT-солвер. Доказанные контракты стираются в release-сборке — нулевая
цена в runtime. Недоказанные откатываются до runtime-assert'а в debug.

Spec: [D24](../spec/decisions/09-tooling.md#d24-стратегия-smt-проверки-контрактов)
(SMT-стратегия) ·
[D111](../spec/decisions/09-tooling.md#d111-assume--assert_static--trusted-external)
(`assume` / `assert_static` / `#trusted`) ·
[D112](../spec/decisions/09-tooling.md#d112-bounded-quantifiers-forallexists-по-коллекции)
(bounded quantifiers) ·
[D116](../spec/decisions/09-tooling.md#d116-z3-backend-через-собственные-ffi-биндинги)
(Z3 backend).

---

## Содержание

- [Quickstart](#quickstart)
- [Клаузулы контракта](#клаузулы-контракта)
  - [`requires`](#requires)
  - [`ensures` и `result`](#ensures-и-result)
  - [`old(...)` в `ensures`](#old-в-ensures)
  - [`decreases`](#decreases)
- [Атрибуты верификации](#атрибуты-верификации)
  - [`#verify`](#verify)
  - [`#pure`](#pure)
  - [`#unverified`](#unverified)
  - [`#must_verify`](#must_verify)
  - [`#trusted`](#trusted)
- [Композиция `#pure`-функций](#композиция-pure-функций)
- [Вспомогательные шаги доказательства](#вспомогательные-шаги-доказательства)
  - [`assert_static`](#assert_static)
  - [`assume`](#assume)
  - [`calc { ... }`](#calc--)
- [Loop invariants](#loop-invariants)
- [Леммы и `apply`](#леммы-и-apply)
- [Opaque-функции и `reveal`](#opaque-функции-и-reveal)
  - [`#opaque`](#opaque)
  - [`reveal fn_name`](#reveal-fn_name)
  - [`#fuel(n)`](#fueln)
- [Bounded quantifiers](#bounded-quantifiers)
- [Битовые векторы и переполнение](#битовые-векторы-и-переполнение)
  - [`#nooverflow`](#nooverflow)
- [Доверенные внешние функции](#доверенные-внешние-функции)
- [Выбор SMT-бэкенда](#выбор-smt-бэкенда)
- [Cross-check верификация (Z3 ↔ CVC5)](#cross-check-верификация-z3--cvc5)
- [Грамматика контрактов](#грамматика-контрактов)
- [Справочник ошибок](#справочник-ошибок)
- [Bootstrap-ограничения](#bootstrap-ограничения)
- [Связанные документы](#связанные-документы)

---

## Quickstart

```nova
// Простое precondition + postcondition.
#verify
fn withdraw(balance int, amount int) -> int
    requires amount > 0 && amount <= balance
    ensures  result == balance - amount
    ensures  result >= 0
{
    balance - amount
}

test "contracts quickstart: withdraw" {
    assert(withdraw(100, 30) == 70)
    assert(withdraw(50, 50)  == 0)
}
```

```nova
// REQUIRES_SMT_BACKEND z3

// Opaque helper + reveal в caller'е — Z3 доказывает более сильный контракт.
#opaque #pure
fn double(x int) -> int
    requires x >= 0
    ensures  result >= 0
=> x * 2

#verify
fn caller_with_reveal(n int) -> int
    requires n >= 0
    ensures  result == n * 2
{
    reveal double
    double(n)
}

test "contracts quickstart: opaque + reveal" {
    assert(double(5) == 10)
    assert(caller_with_reveal(7) == 14)
}
```

---

## Клаузулы контракта

Клаузулы контракта располагаются между списком параметров и `{` телом
(или `=>` expression-body). Несколько клаузул одного вида разрешены и
соединяются конъюнкцией.

### `requires`

Предусловие. SMT-солвер **предполагает** его выполнение при верификации
тела. Caller обязан его соблюсти.

```nova
#verify
fn safe_div(a int, b int) -> int
    requires b != 0
    ensures  result * b == a - (a % b)
{
    a / b
}
```

Несколько `requires`-клаузул эквивалентны одной конъюнкции:

```nova
#verify
fn clamp(x int, lo int, hi int) -> int
    requires lo <= hi
    ensures  result >= lo && result <= hi
{
    if x < lo { lo } else if x > hi { hi } else { x }
}
```

### `ensures` и `result`

Постусловие. `result` ссылается на возвращаемое значение функции.
Несколько `ensures`-клаузул проверяются независимо.

```nova
#verify
fn abs_val(x int) -> int
    ensures result >= 0
    ensures result == x || result == -x
{
    if x >= 0 { x } else { -x }
}
```

### `old(...)` в `ensures`

`old(expr)` захватывает значение выражения **в точке входа** в функцию,
до выполнения тела. Полезно для контрактов с мутацией.

```nova
#verify
fn increment(mut n int) -> int
    ensures result == old(n) + 1
{
    n = n + 1
    n
}
```

### `decreases`

Доказывает терминацию рекурсивных функций. Выражение должно **строго
убывать** при каждом рекурсивном вызове. SMT-солвер проверяет это как
well-foundedness obligation.

```nova
fn factorial(n int) -> int
    requires n >= 0
    decreases n
=> if n == 0 { 1 } else { n * factorial(n - 1) }

fn fib(n int) -> int
    requires n >= 0
    decreases n
=> if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
```

---

## Атрибуты верификации

### `#verify`

Помечает функцию для SMT-верификации. Компилятор кодирует тело и все
контракты как SMT-запрос и спрашивает солвер. Если солвер доказал
контракты — они стираются в release. Если нет — warning `W2402` +
runtime fallback в debug.

```nova
#verify
fn sum_nonneg(a int, b int) -> int
    requires a >= 0
    requires b >= 0
    ensures  result >= 0
{
    a + b
}
```

### `#pure`

Помечает функцию как **чистую** — без side effects, без эффектов в
effect-row. Чистые функции можно свободно вызывать внутри контрактных
выражений (`requires`/`ensures`/`invariant`), где вызовы с эффектами
запрещены.

```nova
#pure
fn is_positive(x int) -> bool => x > 0

#verify
fn safe_log(x int) -> int
    requires is_positive(x)    // вызов #pure в контракте разрешён
    ensures  result >= 0
{
    x - 1
}
```

### `#unverified`

Отказ от SMT-верификации. Контракты остаются как **runtime-assert'ы**
в debug; в release пропускаются. Используйте для контрактов, которые
солвер не может обработать (нелинейная арифметика, строки и т.д.).

```nova
#unverified
fn safe_double(x int) -> int
    requires x > 0
    ensures  result == x * 2
=> x * 2
```

### `#must_verify`

Противоположность `#unverified`. Если SMT-солвер не может доказать
контракт за отведённый таймаут — компиляция **падает** с ошибкой (без
runtime fallback). Используйте для критичного кода.

```nova
#must_verify
fn transfer_total(from_bal int, to_bal int, amount int) -> int
    requires amount > 0 && amount <= from_bal
    ensures  result == from_bal + to_bal
{
    (from_bal - amount) + (to_bal + amount)
}
```

### `#trusted`

Используется в двух контекстах:

**1. `with #trusted`** на binding handler'а — пропускает верификацию
аксиом для этого handler'а, принимает контракты как аксиомы на доверии:

```nova
with #trusted Log = handler Log {
    Write(msg) { if msg > 0 { buf = msg } else { buf = 0 } }
    last() => buf
} { ... }
```

**2. `#trusted` на функции** с `assume` — подавляет предупреждение
`trust-introduced`:

```nova
#trusted
fn call_ffi() -> int {
    let result = extern_fn()
    assume result >= 0    // задокументированный постусловие FFI
    result
}
```

---

## Композиция `#pure`-функций

`#pure`-функции свободно компонуются в контрактных выражениях.
Позволяет создавать переиспользуемые предикаты:

```nova
#pure
fn in_range(x int, lo int, hi int) -> bool => x >= lo && x <= hi

#verify
fn clamp_tight(x int) -> int
    ensures in_range(result, 0, 100)
{
    if x < 0 { 0 } else if x > 100 { 100 } else { x }
}
```

Non-pure функция в контракте — ошибка компиляции:

```
error: effectful function call in contract expression
  contracts require #pure or side-effect-free expressions
```

---

## Вспомогательные шаги доказательства

### `assert_static`

Вставляет **промежуточный шаг доказательства**, видимый SMT-солверу.
Разбивает сложный контракт на маленькие, независимо проверяемые факты.
В debug — runtime check; в release — стирается после доказательства.

```nova
#verify
fn transfer(from int, to int, amount int) -> int
    requires amount > 0 && amount <= from
    ensures  result == from + to
{
    assert_static from - amount >= 0    // промежуточный факт
    (from - amount) + (to + amount)
}
```

### `assume`

Инжектирует факт в SMT-контекст **без доказательства**. Используйте
для постусловий FFI или OS-инвариантов, которые солвер не видит.
Генерирует предупреждение `trust-introduced` вне `#trusted`-функции.

```nova
#trusted
fn read_positive_from_device() -> int {
    let v = device_read()
    assume v >= 0    // задокументированная аппаратная гарантия
    v
}
```

### `calc { ... }`

Структурированная **цепочка равенств** (или неравенств), направляющая
SMT-солвер по шагам. Каждый шаг `== expr;` утверждает равенство с
предыдущей строкой. Солвер проверяет каждый шаг независимо.

```nova
#verify
fn double_is_double(x int) -> int
    ensures result == x * 2
{
    calc {
        x * 2;
        == x * 2;
    }
    x * 2
}
```

Более сложные цепочки могут включать алгебраические тождества:

```nova
#verify
fn add_assoc_proof(a int, b int, c int) -> bool
    ensures result == true
{
    calc {
        (a + b) + c;
        == a + (b + c);    // ассоциативность — Z3 доказывает каждый шаг
    }
    true
}
```

---

## Loop invariants

Клаузула `invariant` внутри тела цикла утверждает условие, которое
выполняется **при каждом входе в итерацию**. SMT-солвер проверяет:
1. Инвариант выполняется перед циклом (инициализация).
2. Если инвариант выполняется в начале итерации и условие цикла
   выполняется, то инвариант выполняется в конце тела (индуктивный шаг).

```nova
// REQUIRES_SMT_BACKEND z3

#verify
fn sum_nonneg_array(n int) -> int
    requires n >= 0
    ensures  result >= 0
{
    let mut sum = 0
    let mut i = 0
    while i < n {
        invariant sum >= 0
        invariant i >= 0
        sum = sum + i
        i = i + 1
    }
    sum
}
```

Клаузула `decreases` также может использоваться в цикле для
доказательства терминации:

```nova
#verify
fn countdown(n int) -> int
    requires n >= 0
    ensures  result == 0
{
    let mut k = n
    while k > 0 {
        invariant k >= 0
        decreases k
        k = k - 1
    }
    k
}
```

---

## Леммы и `apply`

**Лемма** — `#verify`-функция, назначение которой — установить
математический факт: она существует ради своего доказательства, а не
runtime-значения. Обычно возвращает `bool` с `ensures result == true`.

```nova
// REQUIRES_SMT_BACKEND z3

#verify
lemma add_comm(a int, b int) -> bool
    ensures result == true
{
    a + b == b + a
}
```

Оператор `apply` инжектирует постусловие леммы как факт в текущий
SMT-контекст. Позволяет выстраивать цепочки результатов лемм:

```nova
#verify
fn use_commutativity(a int, b int) -> int
    requires a >= 0 && b >= 0
    ensures  result == b + a
{
    apply add_comm(a, b)    // инжектирует: a + b == b + a
    a + b
}
```

**Правила:**
- `apply` работает только внутри `#verify`-функций.
- Лемма должна быть уже доказана (т.е. `#verify` и её контракты
  проверены без ошибки).
- Дублирующий `apply` одной и той же леммы в той же области — warning
  `W2402`.

---

## Opaque-функции и `reveal`

### `#opaque`

`#opaque` на `#pure`-функции скрывает её тело от SMT-солвера. Солвер
трактует её как **неинтерпретированную функцию** (UF): знает
`requires`/`ensures`-контракты, но не реализацию.

Это предотвращает расходимость matching-loop'а в рекурсивных функциях
и даёт контроль над тем, какие caller'ы получают доступ к body-level
proof:

```nova
// REQUIRES_SMT_BACKEND z3

#opaque #pure
fn double(x int) -> int
    requires x >= 0
    ensures  result >= 0
=> x * 2
```

Без `reveal` caller может использовать только задекларированный
`ensures` (result ≥ 0), но не то, что `result == x * 2`:

```nova
// EXPECT_COMPILE_ERROR contract violation

#verify
fn caller_no_reveal(n int) -> int
    requires n >= 0
    ensures  result == n * 2    // Z3 не может доказать — тело скрыто
{
    double(n)
}
```

### `reveal fn_name`

`reveal fn_name` инжектирует body-аксиому `#opaque`-функции в текущую
SMT-область. После `reveal` солвер может использовать полное тело для
доказательств в этой функции:

```nova
// REQUIRES_SMT_BACKEND z3

#verify
fn caller_with_reveal(n int) -> int
    requires n >= 0
    ensures  result == n * 2
{
    reveal double       // инжектируется body-аксиома: double(x) == x * 2
    double(n)
}
```

**Область действия:** `reveal` локален для функции. Другие caller'ы
не затрагиваются.

**Предупреждения:**
- `W2402` — `reveal` в не-`#verify`-функции (нет SMT-контекста).
- `W2402` — дублирующий `reveal` для одного имени в той же области.
- `W2403` — `reveal` для функции, которая не является `#opaque`.

### `#fuel(n)`

`#fuel(n)` на `#opaque #pure`-рекурсивной функции включает **N уровней
разворачивания** в SMT-области после `reveal`. Без fuel opaque body
axiom — нерекурсивная. С `#fuel(2)` солвер получает два уровня
разворачивания — достаточно для доказательства свойств маленьких
конкретных входов:

```nova
// REQUIRES_SMT_BACKEND z3

#opaque #pure #fuel(2)
fn count_down(n int) -> int
    requires n >= 0
    ensures  result >= 0
=>
    if n == 0 { 0 } else { 1 + count_down(n - 1) }

#verify
fn prove_base_case() -> int
    ensures result == 0
{
    reveal count_down
    count_down(0)      // fuel разворачивает: count_down(0) == 0
}

#verify
fn prove_one_step() -> int
    ensures result == 1
{
    reveal count_down
    count_down(1)      // fuel разворачивает: 1 + count_down(0) == 1
}
```

Fuel chain создаёт N промежуточных UF и связывает их аксиомами по
примеру подхода Dafny.

---

## Bounded quantifiers

Nova поддерживает **bounded quantifiers** — `forall`/`exists` по
конкретным коллекциям или индексным диапазонам. Unbounded universal
quantifiers — ошибка компиляции.

```nova
// REQUIRES_SMT_BACKEND z3

#verify
fn all_nonneg_sum(a int, b int, c int) -> bool
    requires a >= 0 && b >= 0 && c >= 0
    ensures  result == true
{
    a + b + c >= 0
}
```

Синтаксис bounded quantifiers в контрактах:

```nova
// forall — универсальный
requires forall i in 0..xs.len() : xs[i] >= 0

// exists — экзистенциальный
ensures  exists i in 0..result.len() : result[i] == target
```

Коллекция после `in` должна быть итерируемой (`[]T`, range, set,
map). Тело должно быть `bool` и `#pure`.

---

## Битовые векторы и переполнение

Sized-integer типы — `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32` —
кодируются в SMT-теорию **битовых векторов** вместо unbounded-целых.
Это даёт точную машинную семантику: арифметика переполняется по модулю
(дополнительный код), битовые операции рассуждаются точно.

```nova
// REQUIRES_SMT_BACKEND z3

#verify
fn low_byte(x u32) -> u32
    ensures result <= 255 as u32
=> x & 255 as u32
```

Тип `int` остаётся **unbounded** математическим целым — это не битовый
вектор. Используйте `int` для general-purpose арифметики; sized-типы —
для low-level, packed, crypto или FFI-кода, где важна разрядность.

**Переполнение `int` — это паника.** Знаковая `int`-арифметика (`+`,
`-`, `*`), выходящая за 64-битный диапазон, **паникует** в рантайме —
она никогда не переполняется молча. Именно это делает верификацию
`int`-контрактов sound: верификатор рассуждает об `int` как о
безграничном математическом целом, и доказанный `ensures result == a + b`
выполняется для каждого значения, которое функция реально возвращает —
потому что при переполнении `a + b` функция паникует, а не возвращает
ошибочный (обёрнутый) результат. Sized-типы вместо паники переполняются
по модулю (см. выше); для них применяйте `#nooverflow`, когда
wrap-around недопустим.

Битовые операторы `&`, `|`, `^`, `<<`, `>>` доступны в контрактах для
sized-integer операндов (на `int` они по-прежнему не поддерживаются).

**Знаковость.** Беззнаковые типы (`u8`/`u16`/`u32`/`u64`) и знаковые
(`i8`/`i16`/`i32`) различаются в сравнении, делении, остатке и сдвиге
вправо. Верификатор выбирает правильный оператор по типу параметра:
сравнения `i32` знаковые (`-1 < 0` истинно), сравнения `u32`
беззнаковые (`0xFFFFFFFF > 0`). Знаковое деление округляет к нулю; `>>`
для знакового значения — арифметический сдвиг.

**Касты между sized-типами.** `x as u32` переразрядивает битовый вектор:
более широкая цель zero-extend'ит беззнаковый источник и sign-extend'ит
знаковый; более узкая — отбрасывает старшие биты. Например `(b as u32)`
где `b : u8` всегда `<= 255`, а `(x as u8)` оставляет только младший байт.

### `#nooverflow`

По умолчанию арифметика sized-целых **переполняется** молча. Атрибут
`#nooverflow` заставляет верификатор генерировать дополнительное
proof-обязательство для каждого `+`, `-`, `*` в теле функции: операция
не должна переполнять тип. Недоказуемое обязательство — ошибка
компиляции.

```nova
// REQUIRES_SMT_BACKEND z3

#nooverflow #verify
fn safe_add_u32(a u32, b u32) -> u32
    requires a <= 1000 as u32 && b <= 1000 as u32
    ensures  result == a + b
=> a + b
```

Здесь предусловие ограничивает `a` и `b`, так что их сумма не превысит
`2^32 - 1` — обязательство переполнения доказано. Без ограничивающего
`requires` `a + b` могло бы переполниться и `#nooverflow` отвергнет
функцию на этапе компиляции.

`#nooverflow` требует SMT-бэкенд с поддержкой битовых векторов
(`REQUIRES_SMT_BACKEND z3`); тривиальный бэкенд сообщает теорию битовых
векторов как неподдерживаемую.

---

## Доверенные внешние функции

`external fn` с контрактами требует `#trusted`. Контракты регистрируются
как **аксиомы** — caller'ы получают `ensures` как предположения без
доказательства. Компилятор не верифицирует тело (Nova-тела нет).

```nova
#trusted
external fn libc_strlen(s str) -> int
    requires s.is_valid_cstring()
    ensures  result >= 0

#verify
fn use_strlen(s str) -> int
    requires s.is_valid_cstring()
    ensures  result >= 0
{
    libc_strlen(s)    // ensures из #trusted-аксиомы инжектируется
}
```

---

## Выбор SMT-бэкенда

Nova имеет два бэкенда верификации:

| Бэкенд | Активируется | Возможности |
|---|---|---|
| **Trivial** | по умолчанию | Constant-folding, линейные bounds на единичных binary ops. Быстрый, без зависимости Z3. |
| **Z3** | env `NOVA_SMT_BACKEND=z3`, либо флаг `--backend z3` у `nova contracts verify` | Полный LIA + EUF + bounded arrays. Обязателен для opaque/reveal, сложных арифметических цепочек, loop invariants. |

Тесты, требующие Z3, используют маркер `// REQUIRES_SMT_BACKEND z3` —
test runner пропускает их при отсутствии Z3.

Таймаут на функцию: по умолчанию 2 секунды. Переопределить локально:

```nova
#verify_timeout(10000)
#verify
fn complex_proof(x int) -> int
    ...
```

---

## Cross-check верификация (Z3 ↔ CVC5)

Cross-check — это **CI-only защитная сеть для soundness**: каждая
verification condition прогоняется через два *независимых* пути
решателя, и при расхождении их определённых ответов сборка падает. Это
вторая линия защиты после soundness-regression-suite (Plan 33.8 Ф.7):
regression-suite ловит *известные* классы багов, cross-check —
*неизвестные*.

Два пути намеренно независимы:

- **Z3** — через FFI-backend.
- **CVC5** — через *текстовый* SMT-LIB v2 скрипт, скармливаемый
  бинарнику `cvc5` подпроцессом.

Текстовый путь не разделяет код с Z3-FFI-трансляцией, поэтому он ещё и
второй независимый *кодировщик*. Баг кодирования, молча терявший
формулу на стороне Z3 (класс багов из Plan 33.8 Ф.6.2), был бы пойман
здесь даже без второго решателя.

### Как запустить

```sh
# Соберите с Z3-backend, поставьте cvc5 в PATH (либо укажите NOVA_CVC5
# на бинарник), затем:
NOVA_CROSSCHECK=1 nova test . --filter contracts
```

`NOVA_CROSSCHECK=1` имеет приоритет над `NOVA_SMT_BACKEND`. Обычная
компиляция (`nova build` / `nova check`) **не затрагивается** — она
использует один решатель, время компиляции разработчика не растёт.

Если `cvc5` не найден, прогон gracefully вырождается в «только Z3» с
warning'ом — cross-check просто не происходит, сборка не ломается.

### Что считается расхождением

Gate срабатывает только на **definite**-расхождении: один путь сказал
`Proven` (unsat), другой — `Disproved` (sat). Любой `Unknown` / timeout
с любой стороны — норма (у решателей разные перф-профили), **не** ошибка.

Расхождение сообщается как ошибка компиляции `E2412` с функцией, VC,
обоими вердиктами, контрпримером и SMT-LIB-скриптом для ручного
воспроизведения. Это soundness-критично: один из путей дал неверный
ответ, значит верификатор мог объявить ложный `Proven`.

### CI-gate

Workflow `contracts-crosscheck` прогоняет весь корпус контрактов под
`NOVA_CROSSCHECK=1` и требует **0 расхождений** для merge.
`NOVA_CROSSCHECK_LOG=<файл>` заставляет каждое расхождение дописывать
строку в этот файл (корпус компилируется процесс-на-файл, поэтому файл —
точка межпроцессной агрегации, которую проверяет gate).

---

## Грамматика контрактов

```
contract-clause  = requires-clause
                 | ensures-clause
                 | decreases-clause

requires-clause  = 'requires' bool-expr
ensures-clause   = 'ensures'  bool-expr
decreases-clause = 'decreases' expr

fn-contracts     = contract-clause*

loop-invariant   = 'invariant' bool-expr
loop-decreases   = 'decreases' expr

calc-block       = 'calc' '{' calc-step+ '}'
calc-step        = expr ';'
               | ('==' | '<=' | '>=' | '<' | '>') expr ';'

reveal-stmt      = 'reveal' ident
apply-stmt       = 'apply' ident '(' expr-list ')'
assert-static    = 'assert_static' bool-expr
assume-stmt      = 'assume' bool-expr

quantifier-expr  = 'forall' ident 'in' expr ':' bool-expr
                 | 'exists' ident 'in' expr ':' bool-expr

old-expr         = 'old' '(' expr ')'
result-ref       = 'result'                  // только в ensures
```

**Сводка атрибутов:**

| Атрибут | На | Значение |
|---|---|---|
| `#verify` | fn | Включить SMT-верификацию |
| `#pure` | fn | Чистая (нет эффектов), используется в контрактах |
| `#unverified` | fn | Пропустить SMT, оставить как runtime check |
| `#must_verify` | fn | Требовать SMT-доказательство — ошибка компиляции если недоказуемо |
| `#trusted` | fn / `with` binding | Принять контракты как аксиомы без доказательства |
| `#opaque` | `#pure` fn | Скрыть тело от SMT; требуется `reveal` для раскрытия |
| `#fuel(n)` | `#opaque #pure` fn | N уровней рекурсивного разворачивания после `reveal` |
| `#verify_timeout(ms)` | `#verify` fn | Переопределить таймаут SMT на функцию |

---

## Справочник ошибок

| Код | Сообщение | Причина |
|---|---|---|
| `W2401` | `contract not verified statically` | SMT вернул Unknown или timeout; откат на runtime check |
| `W2402` | `unverified: ...` | Разное: мёртвая лемма, дублирующий apply/reveal, reveal вне verify-контекста |
| `W2403` | `opaque: ...` | `reveal` для не-opaque fn, `#fuel(0)`, мёртвый `#opaque` (ни разу не reveal'ился) |
| `E2401` | `unsupported expression in contract` | Вызов с эффектом, match, lambda или не-`#pure` в контрактной позиции |
| `E2402` | `contract violation` | SMT опроверг контракт (нашёл контрпример) |
| `E2412` | `cross-check disagreement` | Z3 и CVC5 дали противоположные определённые вердикты для VC (только в cross-check режиме) |
| `trust-introduced` | warning | `assume` вне `#trusted`-контекста |

---

## Bootstrap-ограничения

| Что не работает / отложено | План |
|---|---|
| `#must_verify_module` — strict mode для всего модуля | [D113](../spec/decisions/09-tooling.md#d113) (Plan 33.3 Ф.13, V2) |
| SMT cache + инкрементальная верификация | [D114](../spec/decisions/09-tooling.md#d114) (V2) |
| Параллельная верификация через `rayon` | [D114](../spec/decisions/09-tooling.md#d114) (V2) |
| Loop invariants с Z3 — полное индуктивное рассуждение | Plan 33.x V2 |
| `forall`/`exists` в loop invariants | Plan 33.x V2 |
| Effect-aware контракты (`ensures Db.balance(...) == ...`) | [D24](../spec/decisions/09-tooling.md#d24) / [D120](../spec/decisions/04-effects.md#d120) (частично в V1) |
| Рекурсивные `lemma`-тела (структурная индукция) | Research / V3 |
| Нелинейная арифметика в контрактах | Z3 иногда справляется; статической гарантии нет |
| Рассуждения о floating-point | Не планируется |
| Строковые предикаты сложнее `len()` и equality | Не планируется для V1 |
| `#fuel(0)` — warning (`W2403`), используйте без `#fuel` | По дизайну |

---

## Связанные документы

- [`spec/decisions/09-tooling.md`](../spec/decisions/09-tooling.md) —
  D24 / D89 / D111 / D112 / D113 / D114 / D116 (контракты, SMT, test tooling)
- [`spec/decisions/04-effects.md`](../spec/decisions/04-effects.md) —
  D120 (`#pure` views + axioms), D115 (axiom binders)
- [`docs/plans/33.9-opaque-reveal-fuel.md`](plans/33.9-opaque-reveal-fuel.md) —
  реализация `#opaque` / `reveal` / `#fuel(n)` (Plan 33.9)
- [`docs/plans/33.14-z3-cvc5-crosscheck.md`](plans/33.14-z3-cvc5-crosscheck.md) —
  реализация Z3 ↔ CVC5 cross-check (Plan 33.14)
- [`nova_tests/contracts/`](../nova_tests/contracts/) —
  ~280 тестов верификации контрактов
- [`nova_tests/doc/f23_contracts_positive.nv`](../nova_tests/doc/f23_contracts_positive.nv) —
  базовый doc-пример контрактов
- [`nova_tests/doc/f24_infer_contracts_positive.nv`](../nova_tests/doc/f24_infer_contracts_positive.nv) —
  doc-пример инферированных контрактов
- [`nova_tests/doc/f25_mutation_contracts_positive.nv`](../nova_tests/doc/f25_mutation_contracts_positive.nv) —
  doc-пример контрактов с мутацией
- [`nova_tests/expected_runtime/`](../nova_tests/expected_runtime/) —
  тесты нарушений контракта в runtime (`contracts_*.nv`)
