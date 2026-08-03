---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

# Контракты и формальная верификация в Nova

[English](contracts.md) | **Русский**

Система контрактов Nova позволяет описать, что функция **требует** и
**гарантирует**, и проверяет эти утверждения на этапе компиляции через
SMT-решатель. Модель — **enforce-with-elision** (D24 / Plan 140):
контракты проверяются всегда, компилятор вырезает лишь доказанные
проверки, — а *не* debug-only `assert`-проверки: **доказанный** контракт
элидируется (нулевая цена в рантайме, даже в debug); **недоказанный** —
применяется в рантайме и в debug, и в release (немедленное аварийное
завершение `nova_contract_violation`, без тихого UB). Снять проверку с
недоказанного можно только явно — на функции `#unchecked` или политикой
сборки `--contracts=off`. У отключения проверок три уровня (Plan 140.3):
`#unchecked` на **функции**, `#unchecked` на **модуле** (перед `module X`)
или флаг сборки; плюс гранулярность в стиле Eiffel по видам —
`#unchecked(requires)` / `#unchecked(ensures)` / `#unchecked(invariant)`
(комбинируемо, на функции или модуле) элидируют только перечисленные
виды. Без SMT-бэкенда множество доказанных пусто — проверяется каждый
контракт (безопасное ухудшение: медленнее, но не опасно).

Нарушение контракта — как и провал `assert` — **класса паники**: пойманное
областью видимости `consume`/`supervised`, оно классифицируется как `Panic`,
а не обрабатываемая `Failure` (Plan 140.3 / D13). Сообщение `requires` может
**интерполировать значения времени выполнения** через `${...}` —
`requires x > 0, "got ${x}"` показывает `got -5` на провалившемся вызове
(сообщение строится только при нарушении, не на успешном пути; Plan 140.3).

Спецификация: [D24](../../spec/decisions/09-tooling.md#d24-стратегия-smt-проверки-контрактов)
(стратегия SMT) ·
[D111](../../spec/decisions/09-tooling.md#d111-assume--assert_static--trusted-external)
(`assume` / `assert_static` / `#trusted`) ·
[D112](../../spec/decisions/09-tooling.md#d112-bounded-quantifiers-forallexists-по-коллекции)
(ограниченные кванторы) ·
[D116](../../spec/decisions/09-tooling.md#d116-z3-backend-через-собственные-ffi-биндинги)
(Z3-бэкенд).

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
// Simple precondition + postcondition.
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

// Opaque helper + reveal in caller — Z3 proves the stronger contract.
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
(или `=>` телом-выражением). Несколько клаузул одного вида разрешены и
соединяются конъюнкцией.

### `requires`

Предусловие. SMT-решатель **предполагает** его выполнение при верификации
тела. Вызывающая сторона обязана его соблюсти.

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

#### Диапазон: используйте `&&`, а не цепочку

Чтобы ограничить значение полуинтервалом, пишите каноническую конъюнкцию
`lo <= i && i < hi` — **НЕ** `lo <= i < hi`:

```nova
fn at(buf []int, i int) -> int
    requires 0 <= i && i < buf.len     // ✓ a real bounds check
=> buf[i]
```

Цепочка сравнений `0 <= i < hi` — **ошибка компиляции** (`E_CMP_CHAIN_UNSUPPORTED`):
иначе бы парсилось как `(0 <= i) < hi` = `bool < hi` (вакуумно-истинно — проверка
границ молча превращается в пустую операцию). Nova отвергает цепочку (и
bool/unit-операнды `<` `<=` `>` `>=`, `E_RELATIONAL_OPERAND_NOT_ORDERED`) на этапе
разбора/проверки; пишите через `&&` (Plan 150 / D248).

### Self-access (`@field`, `@len()`) в контрактах метода

Контракт **метода** может ссылаться на состояние получателя (receiver):
читать поле через `@field` или встроенный аксессор размера `@len()` /
`@cap()` / `@byte_len()` / `@is_empty()` (форма вызова взаимозаменяема с
полем: `@len()` ≡ `@len`):

```nova
fn Vec[T] @index(i int) -> T
    requires 0 <= i && i < @len     // ✓ refers to the receiver's length
{
    unsafe { @data[i] }
}
```

SMT-решатель моделирует получателя как сущность `_self`; каждый `@field`
становится неинтерпретированной `_field_<name>(_self)`, поэтому `@len` в
`requires` и `@len` в `ensures` — один и тот же терм (согласованное
рассуждение). Разрешено только **чтение** — контракт это выражение,
записать поле в нём негде.

Когда такой контракт недоказан и срабатывает в рантайме, сообщение о
нарушении отображает self-access-выражение **читаемо** —
`requires failed: 0 <= i && i < @len` — называя реальное поле, а не
заполнитель (Plan 140.2 / D256 §Диагностика).

Вызов ЛЮБОГО метода в контракте — `@method()` на получателе или
`obj.method()` на другом значении, включая цепочку (`a.b().c()`) —
кодируется как неинтерпретированная функция (UF), тем же путём без
встраивания, что и компонуемая свободная `#pure`-функция: имя UF — из
метода, получатель — первый аргумент UF. Недоказанное UF-условие — НЕ
ошибка компиляции, оно уходит в обычную проверку в рантайме
(enforce-with-elision), как любой другой недоказанный контракт.

Что по-прежнему ОБЯЗАТЕЛЬНО — вызываемый метод должен быть **чистым**: без
эффектов, без `mut`-получателя, без незавершающейся рекурсии без
`decreases`. Чистота ВЫВОДИТСЯ тем же способом, что и для свободной функции
(SCC-анализ по графу вызовов, атрибут не нужен); эффектный метод в контракте
остаётся ошибкой компиляции. `#pure` для этого НИКОГДА не обязателен — он
нужен на границах, куда вывод не достаёт (`extern fn`), или как
добровольное явное подтверждение.

### Границы как элидируемый контракт (`Vec @index`)

`Vec[T] @index`/`mut @index` несут `requires 0 <= i && i < @len`, поэтому выход
`v[i]` за границы — нарушение контракта. Границы становятся **элидируемым
контрактом** (D257) по той же модели enforce-with-elision, что и любой контракт:

- **доказуемо** в пределах границ — доступ компилируется **без проверки в
  рантайме** (с нулевой ценой);
- **недоказанный** доступ сохраняет проверку и аварийно падает при выходе
  за границы (в debug *и* release) — без тихого UB.

Верификатор доказывает, что доступ в пределах границ, когда индекс
ограничен, например:

```nova
for i in 0 .. v.len() {
    sum = sum + v[i]          // proven: i ∈ [0, v.len()) → check elided
    v[i] = v[i] * 2           // write-back also elided (in-place keeps length)
}
ro s = v[0 .. v.len()]        // slice v[a..b]: 0<=a && a<=b && b<=v.len() proven

fn at(v Vec[int], i int) -> int
    requires 0 <= i && i < v.len()
=> v[i]                       // cross-fn: bound comes from the `requires`
```

Элизия требует SMT-бэкенда (`NOVA_SMT_BACKEND=z3`); без него проверяется каждый
доступ (безопасное ухудшение). Также нужна **инвариантность длины** вектора в
области видимости — вызов, меняющий длину (`push`/`pop`/…), на том же векторе
сохраняет проверку (ради корректности). Для доступа, доказанного только через
`requires`, проверка сохраняется под `--contracts=off` / `#unchecked` (там
`requires` уже не применяется). `@get`/`@first`/`@last` возвращают `Option` и
дают `None` при выходе за границы — у них нет контракта границ.

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
убывать** при каждом рекурсивном вызове. SMT-решатель проверяет это как
обязательство фундированности (well-foundedness).

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
контракты как SMT-запрос и спрашивает решатель. Доказанные контракты
элидируются (с нулевой ценой, и в debug, и в release). Недоказанные —
проверяются в рантайме и в debug, и в release (enforce-with-elision; для
`#verify` недоказуемость это ошибка компиляции, см. ниже).

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

Помечает функцию как **чистую** — без побочных эффектов, без эффектов в
эффект-строке. Чистые функции (и методы) можно свободно вызывать внутри
контрактных выражений (`requires`/`ensures`/`invariant`), где вызовы с
эффектами запрещены.

Этот атрибут НЕ обязателен для чистоты как таковой: она ВЫВОДИТСЯ
автоматически для любой функции/метода с телом без эффектов (SCC-анализ по
графу вызовов — ближайший аналог: `const fn` в Rust). `#pure` — добровольное
явное подтверждение; компилятор никогда не вымогает его как способ
разблокировать композицию в контракте. Атрибут важен на границах, куда
вывод не достаёт — у `extern`/FFI-функции нет Nova-тела для анализа,
поэтому её чистоту (если есть) нужно объявить, а не вывести.

```nova
// No `#pure` needed here — the body is effect-free, purity is inferred.
fn is_positive(x int) -> bool => x > 0

#verify
fn safe_log(x int) -> int
    requires is_positive(x)    // inferred-pure call allowed in contract
    ensures  result >= 0
{
    x - 1
}
```

### `#unverified`

Отказ от SMT-*верификации* (не от принудительной проверки). Контракты
**недоказаны**, поэтому проверяются в рантайме и в debug, и в release
(enforce-with-elision — ничего не элидируется). Используйте для контрактов,
которые решатель не может обработать (нелинейная арифметика, строки и т.д.).
Чтобы снять и проверку в рантайме — `#unchecked` / `--contracts=off`.

```nova
#unverified
fn safe_double(x int) -> int
    requires x > 0
    ensures  result == x * 2
=> x * 2
```

### `#must_verify`

Противоположность `#unverified`. Если SMT-решатель не может доказать
контракт за отведённый таймаут — компиляция **падает** с ошибкой (без
отката в рантайме). Используйте для критичного кода.

```nova
// Before Plan 33.3:  #must_verify fn f(...) ...
// After  Plan 33.3:  #verify      fn f(...) ...   ← use this
#verify
fn transfer_total(from_bal int, to_bal int, amount int) -> int
    requires amount > 0 && amount <= from_bal
    ensures  result == from_bal + to_bal
{
    (from_bal - amount) + (to_bal + amount)
}
```

### `#trusted`

Используется в двух контекстах:

**1. `with #trusted`** на связывании обработчика — пропускает верификацию
аксиом для этого обработчика, принимает контракты как аксиомы на доверии:

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
    ro result = extern_fn()
    assume result >= 0    // documented FFI postcondition
    result
}
```

---

## Композиция чистых функций/методов

Чистые функции и методы свободно компонуются в контрактных выражениях —
вызов свободной функции встраивается (или кодируется UF, если `#opaque`);
вызов метода (`obj.method()`, включая цепочку) ВСЕГДА кодируется UF,
получатель — первый аргумент. Позволяет создавать переиспользуемые
предикаты без `#pure`, пока тело без эффектов (чистота выводится, см.
`#pure` выше):

```nova
fn in_range(x int, lo int, hi int) -> bool => x >= lo && x <= hi

#verify
fn clamp_tight(x int) -> int
    ensures in_range(result, 0, 100)
{
    if x < 0 { 0 } else if x > 100 { 100 } else { x }
}
```

Эффектная функция/метод в контракте — ошибка компиляции:

```
error: calling function `f` in a contract requires it to be pure
  (purity is inferred automatically for effect-free bodies —
  Plan 33.5 SCC inference, no attribute needed); `f` has effects
  and cannot be used in a contract
```

---

## Вспомогательные шаги доказательства

### `assert_static`

Вставляет **промежуточный шаг доказательства**, видимый SMT-решателю.
Разбивает сложный контракт на маленькие, независимо проверяемые факты.
Доказано → элидируется (с нулевой ценой, и в debug, и в release);
недоказано → проверка в рантайме остаётся в debug И в release
(enforce-with-elision).

```nova
#verify
fn transfer(from int, to int, amount int) -> int
    requires amount > 0 && amount <= from
    ensures  result == from + to
{
    assert_static from - amount >= 0    // intermediate fact
    (from - amount) + (to + amount)
}
```

### `assume`

Инжектирует факт в SMT-контекст **без доказательства**. Используйте
для постусловий FFI или OS-инвариантов, которые решатель не видит.
Генерирует предупреждение `trust-introduced` вне `#trusted`-функции.

```nova
#trusted
fn read_positive_from_device() -> int {
    ro v = device_read()
    assume v >= 0    // documented hardware guarantee
    v
}
```

### `calc { ... }`

Структурированная **цепочка равенств** (или неравенств), направляющая
SMT-решатель по шагам. Каждый шаг `== expr;` утверждает равенство с
предыдущей строкой. Решатель проверяет каждый шаг независимо.

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
        == a + (b + c);    // associativity — Z3 proves each step
    }
    true
}
```

---

## Loop invariants

Клаузула `invariant` внутри тела цикла утверждает условие, которое
выполняется **при каждом входе в итерацию**. SMT-решатель проверяет:
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
    mut sum = 0
    mut i = 0
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
    mut k = n
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
значения времени выполнения. Обычно возвращает `bool` с
`ensures result == true`.

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
    apply add_comm(a, b)    // injects: a + b == b + a
    a + b
}
```

**Правила:**
- `apply` работает только внутри `#verify`-функций.
- Лемма должна быть уже доказана (т.е. `#verify` и её контракты
  проверены без ошибки).
- Дублирующий `apply` одной и той же леммы в той же области —
  предупреждение `W2402`.

---

## Opaque-функции и `reveal`

### `#opaque`

`#opaque` на `#pure`-функции скрывает её тело от SMT-решателя. Решатель
трактует её как **неинтерпретированную функцию** (UF): знает
`requires`/`ensures`-контракты, но не реализацию.

Это предотвращает расходимость цикла сопоставления (matching loop) в
рекурсивных функциях и даёт контроль над тем, какие вызывающие стороны
получают доступ к доказательству на уровне тела:

```nova
// REQUIRES_SMT_BACKEND z3

#opaque #pure
fn double(x int) -> int
    requires x >= 0
    ensures  result >= 0
=> x * 2
```

Без `reveal` вызывающая сторона может использовать только задекларированный
`ensures` (result ≥ 0), но не то, что `result == x * 2`:

```nova
// EXPECT_COMPILE_ERROR contract violation

#verify
fn caller_no_reveal(n int) -> int
    requires n >= 0
    ensures  result == n * 2    // Z3 cannot prove — body is hidden
{
    double(n)
}
```

### `reveal fn_name`

`reveal fn_name` инжектирует аксиому тела `#opaque`-функции в текущую
SMT-область. После `reveal` решатель может использовать полное тело для
доказательств в этой функции:

```nova
// REQUIRES_SMT_BACKEND z3

#verify
fn caller_with_reveal(n int) -> int
    requires n >= 0
    ensures  result == n * 2
{
    reveal double       // body axiom injected: double(x) == x * 2
    double(n)
}
```

**Область действия:** `reveal` локален для функции. Другие вызывающие
стороны не затрагиваются.

**Предупреждения:**
- `W2402` — `reveal` в не-`#verify`-функции (нет SMT-контекста).
- `W2402` — дублирующий `reveal` для одного имени в той же области.
- `W2403` — `reveal` для функции, которая не является `#opaque`.

### `#fuel(n)`

`#fuel(n)` на `#opaque #pure`-рекурсивной функции включает **N уровней
разворачивания** в SMT-области после `reveal`. Без `#fuel` аксиома
opaque-тела нерекурсивна. С `#fuel(2)` решатель получает два уровня
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
    count_down(0)      // fuel unrolls: count_down(0) == 0
}

#verify
fn prove_one_step() -> int
    ensures result == 1
{
    reveal count_down
    count_down(1)      // fuel unrolls: 1 + count_down(0) == 1
}
```

Механизм fuel создаёт N промежуточных UF и связывает их аксиомами по
примеру подхода Dafny.

---

## Bounded quantifiers

Nova поддерживает **ограниченные кванторы** — `forall`/`exists` по
конкретным коллекциям или индексным диапазонам. Неограниченные
универсальные кванторы — ошибка компиляции.

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

Синтаксис ограниченных кванторов в контрактах:

```nova
// forall — universal
requires forall i in 0..xs.len() : xs[i] >= 0

// exists — existential
ensures  exists i in 0..result.len() : result[i] == target
```

Коллекция после `in` должна быть итерируемой (`[]T`, диапазон, множество,
отображение). Тело должно быть `bool` и `#pure`.

---

## Битовые векторы и переполнение

Целочисленные типы фиксированной ширины — `u8`, `u16`, `u32`, `u64`, `i8`,
`i16`, `i32` — кодируются в SMT-теорию **битовых векторов** вместо
неограниченных целых. Это даёт точную машинную семантику: арифметика
переполняется по модулю (дополнительный код), битовые операции рассуждаются
точно.

```nova
// REQUIRES_SMT_BACKEND z3

#verify
fn low_byte(x u32) -> u32
    ensures result <= 255 as u32
=> x & 255 as u32
```

Тип `int` остаётся **неограниченным** математическим целым — это не битовый
вектор. Используйте `int` для арифметики общего назначения; типы
фиксированной ширины — для низкоуровневого, упакованного,
криптографического или FFI-кода, где важна разрядность.

**Переполнение `int` — это паника.** Знаковая `int`-арифметика (`+`,
`-`, `*`), выходящая за 64-битный диапазон, **паникует** в рантайме —
она никогда не переполняется молча. Именно это делает верификацию
`int`-контрактов корректной: верификатор рассуждает об `int` как о
безграничном математическом целом, и доказанный `ensures result == a + b`
выполняется для каждого значения, которое функция реально возвращает —
потому что при переполнении `a + b` функция паникует, а не возвращает
ошибочный (обёрнутый) результат. Типы фиксированной ширины вместо паники
переполняются по модулю (см. выше); для них применяйте `#nooverflow`, когда
переполнение по модулю недопустимо.

**Доказуемо безопасные проверки переполнения элидируются.** Каждый `int`
`+`/`-`/`*` компилируется в постоянно включённую проверку переполнения
(`nova_int_checked_*`). Когда Z3-бэкенд доказывает, что результат остаётся в
64-битном диапазоне — из границ цикла, литералов или `requires` — проверка
**убирается** (с нулевой ценой), ровно как элидируемая проверка границ
(D272, та же модель enforce-with-elision). Ограниченный границами цикла
`i + j` или ограниченный через `requires` `a + b` порождает обычный
C-оператор; недоказанная операция оставляет проверку (в debug *и* release).
Элизия **только по доказательству** — никогда одним лишь `#unchecked`:
проверка, доказанная только через `requires`, остаётся под `--contracts=off`
/ `#unchecked(requires)`. Нужен `NOVA_SMT_BACKEND=z3`; без него проверяется
всё. `*` нелинейна — Z3 может оставить проверку.

Битовые операторы `&`, `|`, `^`, `<<`, `>>` доступны в контрактах для
операндов фиксированной ширины (на `int` они по-прежнему не
поддерживаются).

**Знаковость.** Беззнаковые типы (`u8`/`u16`/`u32`/`u64`) и знаковые
(`i8`/`i16`/`i32`) различаются в сравнении, делении, остатке и сдвиге
вправо. Верификатор выбирает правильный оператор по типу параметра:
сравнения `i32` знаковые (`-1 < 0` истинно), сравнения `u32`
беззнаковые (`0xFFFFFFFF > 0`). Знаковое деление округляет к нулю; `>>`
для знакового значения — арифметический сдвиг.

**Приведения между типами фиксированной ширины.** `x as u32` меняет
разрядность битового вектора: более широкая цель расширяет нулями
беззнаковый источник и знаковым расширением знаковый; более узкая —
отбрасывает старшие биты. Например `(b as u32)` где `b : u8` всегда
`<= 255`, а `(x as u8)` оставляет только младший байт.

### `#nooverflow`

По умолчанию арифметика целых фиксированной ширины **переполняется** молча.
Атрибут `#nooverflow` заставляет верификатор генерировать дополнительное
обязательство доказательства для каждого `+`, `-`, `*` в теле функции:
операция не должна переполнять тип. Недоказуемое обязательство — ошибка
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
как **аксиомы** — вызывающие стороны получают `ensures` как предположения
без доказательства. Компилятор не верифицирует тело (Nova-тела нет).

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
    libc_strlen(s)    // ensures from #trusted axiom injected
}
```

---

## Выбор SMT-бэкенда

Nova имеет два бэкенда верификации:

| Бэкенд | Активируется | Возможности |
|---|---|---|
| **Trivial** | по умолчанию | Свёртка констант, линейные границы на одиночных бинарных операциях. Быстрый, без зависимости Z3. |
| **Z3** | env `NOVA_SMT_BACKEND=z3`, либо флаг `--backend z3` у `nova contracts verify` | Полный LIA + EUF + ограниченные массивы. Обязателен для opaque/reveal, сложных арифметических цепочек, инвариантов циклов. |

Тесты, требующие Z3, используют маркер `// REQUIRES_SMT_BACKEND z3` —
исполнитель тестов пропускает их при отсутствии Z3.

Таймаут на функцию: по умолчанию 2 секунды. Переопределить локально:

```nova
#verify_timeout(10000)
#verify
fn complex_proof(x int) -> int
    ...
```

---

## Cross-check верификация (Z3 ↔ CVC5)

Cross-check — это **включаемая только в CI защитная сеть корректности**:
каждое условие верификации прогоняется через два *независимых* пути
решателя, и при расхождении их определённых ответов сборка падает. Это
вторая линия защиты после регрессионного набора корректности (Plan 33.8
Ф.7): регрессионный набор ловит *известные* классы багов, cross-check —
*неизвестные*.

Два пути намеренно независимы:

- **Z3** — через FFI-бэкенд.
- **CVC5** — через *текстовый* SMT-LIB v2 скрипт, скармливаемый
  бинарнику `cvc5` подпроцессом.

Текстовый путь не разделяет код с Z3-FFI-трансляцией, поэтому он ещё и
второй независимый *кодировщик*. Баг кодирования, молча терявший
формулу на стороне Z3 (класс багов из Plan 33.8 Ф.6.2), был бы пойман
здесь даже без второго решателя.

### Как запустить

```sh
# Build with the Z3 backend, install cvc5 on PATH (or point NOVA_CVC5
# at the binary), then:
NOVA_CROSSCHECK=1 nova test . --filter contracts
```

`NOVA_CROSSCHECK=1` имеет приоритет над `NOVA_SMT_BACKEND`. Обычная
компиляция (`nova build` / `nova check`) **не затрагивается** — она
использует один решатель, время компиляции разработчика не растёт.

Если `cvc5` не найден, прогон мягко вырождается в «только Z3» с
предупреждением — cross-check просто не происходит, сборка не ломается.

### Что считается расхождением

Порог срабатывает только на **определённом** расхождении: один путь сказал
`Proven` (unsat), другой — `Disproved` (sat). Любой `Unknown` / таймаут
с любой стороны — норма (у решателей разные профили производительности),
**не** ошибка.

Расхождение сообщается как ошибка компиляции `E2412` с функцией, VC,
обоими вердиктами, контрпримером и SMT-LIB-скриптом для ручного
воспроизведения. Это критично для корректности: один из путей дал неверный
ответ, значит верификатор мог объявить ложный `Proven`.

### CI-gate

Процедура `contracts-crosscheck` в CI прогоняет весь корпус контрактов под
`NOVA_CROSSCHECK=1` и требует **0 расхождений** для слияния.
`NOVA_CROSSCHECK_LOG=<файл>` заставляет каждое расхождение дописывать
строку в этот файл (корпус компилируется по одному процессу на файл,
поэтому файл — точка межпроцессной агрегации, которую проверяет порог).

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
result-ref       = 'result'                  // only in ensures
```

**Сводка атрибутов:**

| Атрибут | На | Значение |
|---|---|---|
| `#verify` | fn | Включить SMT-верификацию |
| `#pure` | fn | Явное объявление чистоты (нет эффектов); используется в контрактах. Добровольно — чистота выводится автоматически для тел без эффектов; нужен только там, куда вывод не достаёт (`extern fn`) |
| `#unverified` | fn | Пропустить SMT, оставить как проверку в рантайме |
| `#must_verify` | fn | Требовать SMT-доказательство — ошибка компиляции если недоказуемо |
| `#trusted` | fn / `with` binding | Принять контракты как аксиомы без доказательства |
| `#opaque` | `#pure` fn | Скрыть тело от SMT; требуется `reveal` для раскрытия |
| `#fuel(n)` | `#opaque #pure` fn | N уровней рекурсивного разворачивания после `reveal` |
| `#verify_timeout(ms)` | `#verify` fn | Переопределить таймаут SMT на функцию |

---

## Справочник ошибок

| Код | Сообщение | Причина |
|---|---|---|
| `W2401` | `contract not verified statically` | SMT вернул Unknown или таймаут; откат на проверку в рантайме |
| `W2402` | `unverified: ...` | Разное: мёртвая лемма, дублирующий apply/reveal, reveal вне контекста `#verify` |
| `W2403` | `opaque: ...` | `reveal` для функции, не являющейся `#opaque`; `#fuel(0)`; мёртвый `#opaque` (ни разу не раскрывался) |
| `E2401` | `unsupported expression in contract` | match, lambda, tuple-литерал или другая конструкция, которую SMT-кодировщик вообще не умеет представить (вызовы — свободная функция или метод — ВСЕГДА кодируемы; эффектный вызываемый — отдельная ошибка «requires it to be pure», не E2401) |
| `E2402` | `contract violation` | SMT опроверг контракт (нашёл контрпример) |
| `E2412` | `cross-check disagreement` | Z3 и CVC5 дали противоположные определённые вердикты для VC (только в cross-check режиме) |
| `trust-introduced` | warning | `assume` вне `#trusted`-контекста |

---

## Bootstrap-ограничения

| Что не работает / отложено | План |
|---|---|
| `#must_verify_module` — строгий режим для всего модуля | [D113](../../spec/decisions/09-tooling.md#d113) (Plan 33.3 Ф.13, V2) |
| SMT кэш + инкрементальная верификация | [D114](../../spec/decisions/09-tooling.md#d114) (V2) |
| Параллельная верификация через `rayon` | [D114](../../spec/decisions/09-tooling.md#d114) (V2) |
| Инварианты циклов с Z3 — полное индуктивное рассуждение | Plan 33.x V2 |
| `forall`/`exists` в инвариантах циклов | Plan 33.x V2 |
| Контракты с учётом эффектов (`ensures Db.balance(...) == ...`) | [D24](../../spec/decisions/09-tooling.md#d24) / [D120](../../spec/decisions/04-effects.md#d120) (частично в V1) |
| Рекурсивные `lemma`-тела (структурная индукция) | Research / V3 |
| Нелинейная арифметика в контрактах | Z3 иногда справляется; статической гарантии нет |
| Рассуждения о числах с плавающей точкой | Не планируется |
| Строковые предикаты сложнее `len()` и равенства | Не планируется для V1 |
| `#fuel(0)` — предупреждение (`W2403`), используйте без `#fuel` | По дизайну |

---

## Связанные документы

- [`spec/decisions/09-tooling.md`](../../spec/decisions/09-tooling.md) —
  D24 / D89 / D111 / D112 / D113 / D114 / D116 (контракты, SMT, инструментарий тестирования)
- [`spec/decisions/04-effects.md`](../../spec/decisions/04-effects.md) —
  D120 (`#pure`-представления + аксиомы), D115 (связыватели аксиом)
- [`docs/plans/33.9-opaque-reveal-fuel.md`](../plans/33.9-opaque-reveal-fuel.md) —
  реализация `#opaque` / `reveal` / `#fuel(n)` (Plan 33.9)
- [`docs/plans/33.14-z3-cvc5-crosscheck.md`](../plans/33.14-z3-cvc5-crosscheck.md) —
  реализация Z3 ↔ CVC5 cross-check (Plan 33.14)
- [`nova_tests/contracts/`](../../nova_tests/contracts/) —
  ~280 тестов верификации контрактов
- [`nova_tests/doc/f23_contracts_positive.nv`](../nova_tests/doc/f23_contracts_positive.nv) —
  базовый пример контрактов из документации
- [`nova_tests/doc/f24_infer_contracts_positive.nv`](../nova_tests/doc/f24_infer_contracts_positive.nv) —
  пример выводимых контрактов из документации
- [`nova_tests/doc/f25_mutation_contracts_positive.nv`](../nova_tests/doc/f25_mutation_contracts_positive.nv) —
  пример контрактов с мутацией из документации
- [`nova_tests/expected_runtime/`](../nova_tests/expected_runtime/) —
  тесты нарушений контракта в рантайме (`contracts_*.nv`)
