<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 237 — `bigfloat` (пакет nova-bigint): двоичная произвольной точности поверх BigInt

**Статус:** 📋 ОФОРМЛЕН 2026-07-30. Реализация НЕ начата. **Приоритет:** P3 (не блокирует v0.1; стек ПОСЛЕ [235](235-bigint.md) и [236](236-bigdecimal.md) — BigInt-фундамент первичен, BigDecimal-конверсии расширяют покрытие).

**Зависимость:** блокирован планами 235 (BigInt — мантисса) и 236 (BigDecimal — конверсии туда/обратно). Вся арифметика BigFloat сводится к BigInt; конверсии в десятичную строку оптимальнее через BigDecimal, чем напрямую.

## 0. Мотив

BigFloat — двоичное число произвольной точности: `mantissa × 2^{exponent}`. Это естественный тип для научных вычислений, численного анализа, и любого кода, где важна **относительная** точность в битах (в отличие от BigDecimal, где точность абсолютная в десятичных знаках).

Отличие от `f64`:
- точность не фиксирована (53 бита мантиссы) — программист выбирает: 128 бит, 256, 1024;
- rounding-explicit: каждая операция принимает `PrecisionContext` (биты точности + rounding mode);
- никакого тихого overflow/underflow к нулю или бесконечности.

BigFloat строится **исключительно поверх BigInt** (Plan 235) — как MPFR поверх GMP. Все арифметические операции — BigInt-свёртки с постраундом. Единственная добавленная сложность — приведение порядка (alignment) и двоичное округление.

Nova-специфика: BigFloat — value-record (`type BigFloat value { mant BigInt, exp int }`). Копия копирует указатель на BigInt (heap-лимбы), а не лимбы. Операторный desugar через `@plus`/`@minus`/`@times` на value-record — прецедент есть (str, BigDecimal). `/` не десугарится (требует PrecisionContext).

## 1. Объём V1 — ТОЛЬКО двоичная арифметика с явной точностью

**V1 = корректность, читаемость, покрытие; никаких оптимизаций.** BigFloat-V1 — обёртка над BigInt, реализующая:

- представление + нормализация (удаление trailing zero bits)
- конверсии `int → BigFloat`, `f64 → BigFloat`, `str → BigFloat`, `BigFloat → str`
- `BigFloat ↔ BigDecimal` (round-trip)
- `@plus`/`@minus`/`@times`/`@neg`/`@abs` — с PrecisionContext (точность + rounding mode)
- `@div` с PrecisionContext (только метод, не оператор `/`)
- `@sqrt` с PrecisionContext (V1: Ньютон + BigInt @div_rem)
- `@compare`/`@equal`
- `@round` (precision-based, значащие биты)
- `@to_f64` (checked, `Option[f64]` — overflow/underflow → None; Inf/NaN → None)
- `@to_i64` / `@to_u64` (checked, `Option[i64]` / `Option[u64]` — overflow/NaN → None)
- `@to_bigint` (truncation: отбрасывает дробную часть; mant-без-exp ≠ округление)
- `@sign` → `Sign` (Neg/Zero/Pos), `is_zero()`, `is_pos()`, `is_neg()`
- `is_integer()` (mant × 2^{exp} — целое? exp ≥ 0 ∨ mant кратно 2^{|exp|})
- Конструкторы: `BigFloat.zero()`, `BigFloat.one()` (статики через `.` — `::` это Rust-синтаксис; `two()` СНЯТ: половинение/удвоение — `exp ∓ 1`, полный `@div` и константа не нужны)

V1 **не** включает: трансцендентные функции (`exp`/`ln`/`sin`/`cos`/`pow`); цепные операции с автоматической точностью; FMA; интеграцию с generic-числовыми type-sets (D310); неявные коэрсии.

## 2. Дизайн (V1)

### 2.1. Представление

```nova
// RoundingMode — тот же enum, что в BigDecimal (236), чтобы был один канон.
// Если BigDecimal ещё не реализован к моменту Ф.1 — определить локально
// (при слиянии обоих — вынести в общий модуль РЕПЫ nova-bigint, напр. src/rounding.nv;
//  НЕ «std math» — дом пакета §2.8, std не трогаем).
type RoundingMode enum HalfEven | HalfUp | HalfDown | Down | Up | Ceiling | Floor
// Семантика — идентична IEEE 754-2019 §4.3:
// HalfEven  — tiesToEven (ближайший, при ровно половине к чётному)
// HalfUp    — tiesToAway (ближайший, при ровно половине от нуля)
// HalfDown  — tiesToZero (ближайший, при ровно половине к нулю)
// Down      — roundTowardZero (truncation)
// Up        — roundAwayFromZero
// Ceiling   — roundTowardPositive
// Floor     — roundTowardNegative

type PrecisionContext value {
    prec int,         // ЗНАЧАЩИХ БИТ мантиссы (ВКЛЮЧАЯ implicit leading 1 для нормализованных);
                      // ИНВАРИАНТ ≥ 2 (иначе любое округление — до 0 или ±∞ по знаку).
                      // IEEE 754 binary64 = 53, binary32 = 24.
    rm RoundingMode,
}

type BigFloat value {
    mant BigInt,      // значение = mant × 2^{exp}; mantissa — ЦЕЛОЕ (fixed-point, точка СПРАВА)
    exp int,          // может быть < 0
}
```

**Value-record:** BigFloat на стеке (копия = 8 байт pointer + 8 байт int). BigInt-мантисса — heap-record, разделяется между копиями (копируется указатель). Иммутабельность BigInt гарантирует отсутствие alias-мутаций.

**Нормализация** (инвариант `normalize()` — ключевое отличие от BigDecimal):
- Если `mant.is_zero()` → `BigFloat { mant: Sign.Zero, exp: 0 }`.
- Иначе: удалить все конечные нулевые БИТЫ из мантиссы, увеличив `exp` на их количество.
  ```nova
  // while mant чётна и не ноль
  while !mant.is_zero() && mant.is_even()   // чётность — дешевле, чем @bitand 1 (имя по D46: @bitand, не @bit_and) {
      mant = mant @div 2        // BigInt сдвиг вправо
      exp = exp + 1
  }
  ```
  Это **дешевле**, чем BigDecimal-normalize (сдвиги битов, а не десятичные деления). BigInt деление на 2 — O(n), где n — число лимбов; вся нормализация BigInt с 2^30 trailing zeros = 30 делений, а не ~9e9 как у BigDecimal.

- Примеры:
  | Значение | mant | exp | normalize → mant | exp |
  |---|---|---|---|---|
  | 6.0 | BigInt(6) | 0 | 3 | 1 |
  | 0.75 (= 3/4) | BigInt(3) | -2 | 3 | -2 |
  | 1.0 | BigInt(1) | 0 | 1 | 0 |

**⚠** Нормализованная форма — `mant` нечётна ИЛИ ноль. Это гарантирует уникальное представление (как IEEE 754 leading-1, но явно: у BigInt нет implicit bit, есть точное целое).

**Lazy normalize:** нормализация НЕ вызывается автоматически после арифметических операций (round_to_precision оставляет mant чётной — это нормально). Вызывается в `@equal`, `@hash` и при явном `normalize()`. V1-инвариант: конструкторы и арифметика не гарантируют нормализованность; `@equal`/`@hash` нормализуют перед работой.

### 2.2. PrecisionContext и rounding — двоичное округление

**Rounding Mode** — идентичен BigDecimal (см. 236 §2.3), но работает на БИТЫ, не на десятичные цифры.

**Округление после операции:**
```
fn round_to_precision(mant_abs: BigInt, prec: int, rm: RoundingMode, sign: Sign, sticky: bool) -> (BigInt, int)
```
BigInt НЕОТРИЦАТЕЛЬНЫЙ. Возвращает `(rounded, bits_lost)` — количество отброшенных битов (нужно для коррекции exp). `sticky` — `true`, если после отброшенного хвоста есть ненулевые биты (нужен для HalfEven/HalfUp — ties-детекции).

```
b = mant_abs.bit_length() - prec           // сколько бит отбросить
if b <= 0 → return (mant_abs, 0)           // уже не длиннее цели
factor = 2^b                                // BigInt: 1 << b
(quot, rem) = mant_abs.div_rem(factor)      // mant_abs >= 0 → rem >= 0
half = factor / 2                            // 2^{b-1}
gt_half = rem > half || (rem == half && sticky)
eq_half = rem == half && !sticky
tail    = rem != 0 || sticky
carry = match rm:
    HalfEven => gt_half || (eq_half && quot.is_odd())
    HalfUp   => gt_half || eq_half
    HalfDown => gt_half
    Down     => false
    Up       => tail
    Ceiling  => sign == Pos && tail
    Floor    => sign == Neg && tail
rounded = quot + (carry ? 1 : 0)
if rounded.bit_length() > prec:              // carry: 111... → 1000...
    rounded = rounded >> 1                   // хвост после carry нулевой
    b += 1
→ (rounded, b)
```

### 2.3. Арифметика — вся через BigInt

**Сложение / вычитание (`@plus`/`@minus`):**
```
// Выравнивание порядков (BigInt << k, работа на абсолютных значениях).
// Знак — Sign-value из BigInt; сдвиги и сложение — беззнаковые BigInt.
GUARD = 3
a_sign = a.mant.sign; a_abs = |a.mant|
b_sign = b.mant.sign; b_abs = |b.mant|
if a.exp < b.exp:
    diff = b.exp - a.exp
    a_wide = a_abs << (diff + GUARD)    // BigInt беззнаковый сдвиг
    b_wide = b_abs << GUARD
    exp = a.exp - GUARD
else:
    diff = a.exp - b.exp
    b_wide = b_abs << (diff + GUARD)
    a_wide = a_abs << GUARD
    exp = b.exp - GUARD
// Знаковое сложение BigInt (abs values, sign отдельно):
if a_sign == b_sign:
    mant_abs = a_wide + b_wide;  sign = a_sign
else:
    if a_wide >= b_wide:
        mant_abs = a_wide - b_wide;  sign = a_sign
    else:
        mant_abs = b_wide - a_wide;  sign = b_sign
(mant, bits_lost) = round_to_precision(mant_abs, ctx.prec, ctx.rm, sign, sticky=false)
exp = exp + bits_lost
→ BigFloat { mant: apply_sign(mant, sign), exp }
```

**⚠ Память при большой разнице порядков (ревью 2026-07-30):** выравнивание точное —
`a_abs << (diff + GUARD)` при `diff = 10⁶` материализует мегабитный BigInt. В `@compare`
эта опасность устранена msb-fast-path (§2.4), в сложении — НЕТ. Точность не страдает
(сумма точная → `sticky=false` корректен), только память/время. V1: документировано;
V2 — MPFR-приём: при `diff > prec + GUARD` меньший операнд не сдвигать, а учесть одним
sticky-битом. (GUARD в сложении, строго говоря, избыточен — сумма вычисляется ТОЧНО и
округляется один раз; оставлен для единообразия с делением.)

**Умножение (`@times`):**
```
mant_abs = |a.mant| × |b.mant|   // BigInt @times: точное
sign = a.mant.sign == b.mant.sign ? Pos : Neg
exp = a.exp + b.exp
(mant_abs, bits_lost) = round_to_precision(mant_abs, ctx.prec, ctx.rm, sign, sticky=false)
exp = exp + bits_lost
→ BigFloat { mant: apply_sign(mant_abs, sign), exp }
```

**Деление (`@div`, ключевая сложность):**
```
// ТРЕБОВАНИЕ: !b.is_zero() — panic (паритет int, D423)
// a/b = (a.mant / b.mant) × 2^{a.exp - b.exp}
// Расширяем делимое на ctx.prec + GUARD битов, делим (BigInt @div_rem), округляем.

a_abs = |a.mant|; b_abs = |b.mant|
da = a_abs.bit_length(); db = b_abs.bit_length()
sign = a.mant.sign == b.mant.sign ? Pos : Neg   // Zero-handling: a или b Zero → Pos
extra = ctx.prec + GUARD - da + db               // сколько битов добавить к делимому
if extra < 0: extra = 0
dividend = a_abs << extra
(quot, rem) = dividend.div_rem(b_abs)
sticky = rem != 0
(mant, bits_lost) = round_to_precision(quot, ctx.prec, ctx.rm, sign, sticky)
result_exp = a.exp - b.exp + extra - bits_lost
→ BigFloat { mant: apply_sign(mant, sign), exp: result_exp }
```

**Neg / Abs (`@neg`/`@abs`):**
```
@neg: BigFloat { mant: -self.mant, exp: self.exp }  // sign flip, без ctx
@abs: if self.sign == Neg → @neg() else → self
```

**Квадратный корень (`@sqrt`):**
```
// Итерация Ньютона: x_{n+1} = (x_n + a/x_n) / 2
// Все итерации — BigFloat арифметика с PrecisionContext.
// Начальное приближение: f64 @to_bigfloat (или a >> 1).
// Итерации до сходимости (x_{n+1} == x_n).
// ТРЕБОВАНИЕ: a >= 0 — иначе panic.

fn BigFloat @sqrt(ctx PrecisionContext) -> BigFloat {
    requires !self.mant.is_neg()  // отрицательное → panic
    if self.is_zero() → BigFloat.zero()

    // Начальное приближение через f64; вне f64-диапазона (None) или при
    // subnormal-нуле — грубое { mant: 1, exp: self.exp / 2 } (см. риски)
    mut prev   = BigFloat.zero()
    mut approx = начальное приближение
    mut iters  = 0
    loop {
        ro quot = self.@div(approx, ctx)
        ro sum  = approx.@plus(quot, ctx)
        // ÷2 = exp - 1: ТОЧНО и бесплатно — полный @div и константа two() не нужны
        ro next = BigFloat { mant: sum.mant, exp: sum.exp - 1 }
        iters += 1
        // СТОП-КРИТЕРИЙ (фикс ревью 2026-07-30): «до строгого равенства» МОЖЕТ НЕ
        // ТЕРМИНИРОВАТЬСЯ — Ньютон при фиксированном prec классически ОСЦИЛЛИРУЕТ
        // между двумя соседними представимыми значениями. Поэтому: равенство с
        // предыдущим ИЛИ позапрошлым значением, плюс жёсткий cap итераций
        // (сходимость квадратичная: ~log2(prec) шагов; cap = log2(prec) + 8).
        if next.@equal(approx) || next.@equal(prev) || iters > log2(ctx.prec) + 8 { break }
        prev = approx; approx = next
    }
    approx
}
```

Сходимость квадратичная — cap не срабатывает на здоровых входах, он страховка от осцилляции.

### 2.4. Сравнение и равенство

**@compare:**
```
// Знак хранится в BigInt (a.mant.sign)
a_s = a.mant.sign; b_s = b.mant.sign
if a_s != b_s:                     // Neg < Zero < Pos
    return порядок знаков
// Знаки равны (и не Zero). Сравниваем ВЕЛИЧИНЫ, инвертируя результат для Neg-пары.
// (Фикс ревью 2026-07-30 — два дефекта прежней редакции:
//  1. fast-path «diff > MAX_SHIFT → больший exp больше» НЕВЕРЕН: величина определяется
//     ПОЗИЦИЕЙ СТАРШЕГО БИТА msb = exp + bit_length(mant), а не exp — 1×2^100 (msb=101)
//     МЕНЬШЕ, чем 50-битная мантисса × 2^60 (msb=110);
//  2. медленный путь сравнивал |mant| БЕЗ инверсии для отрицательной пары: -3 vs -5
//     давал «-3 < -5».)
a_abs = |a.mant|; b_abs = |b.mant|
msb_a = a.exp + a_abs.bit_length(); msb_b = b.exp + b_abs.bit_length()
inv = (a_s == Neg) ? -1 : 1
if msb_a != msb_b: return (msb_a > msb_b ? 1 : -1) × inv    // O(1), БЕЗ сдвигов
// msb равны → сдвиг выравнивания ограничен |bit_length_a - bit_length_b|
// (не больше размера самих мантисс — НЕ взрывается; MAX_SHIFT_FOR_COMPARE снят как ненужный:
//  msb-fast-path покрывает все случаи гигантской разницы порядков за O(1))
diff = |a.exp - b.exp|
if a.exp < b.exp: return a_abs.@compare(b_abs << diff) × inv
else:             return (a_abs << diff).@compare(b_abs) × inv
```

**@equal:** Всегда нормализовать оба операнда перед сравнением (lazy normalize — после операций mant может быть чётной). Затем field-by-field сравнение value-record (одинаковые normalized mant+exp ↔ одно и то же число).

### 2.5. Конверсии

- `fn[T Ints] T @to_bigfloat(ctx PrecisionContext) -> BigFloat` — `T @to_bigint()` (НЕ `BigInt::from` — Rust-синтаксис и ретрактированная статик-конверсия §1а/W_STATIC_CONVERSION), `exp = 0`. В V1 — явные перегрузки для `int`/`i8..i64`/`u8..u64`, generic — если `Ints` доступен.

- `f64 @to_bigfloat() -> BigFloat` — точное представление f64-битов:
  ```
  // Распаковать IEEE 754 binary64: sign(1) + exp(11) + mant(52)
  // Нормализованные: mant_bf = 2^52 + mant_bits, exp_bf = exp_biased - 1023 - 52
  // Субнормальные:   mant_bf = mant_bits, exp_bf = -1022 - 52
  // ±0 → BigFloat.zero() (mant=0, exp=0)
  // ±Inf → panic (V1 не имеет Inf; D423 trap-политика: бесконечность не число)
  // NaN  → panic (D423: не-число не может стать числом)
  ```

- `BigFloat @to_f64() -> Option[f64]` — проверка, помещается ли в f64. Возвращает `None` при overflow/underflow. (Имитация round-to-nearest-ties-to-even IEEE 754.)

- `BigFloat @to_int() -> Option[int]` — fits-проверка (BigInt @to_int на мантиссе с учётом exp).

- `BigFloat @to_i64() -> Option[i64]`, `BigFloat @to_u64() -> Option[u64]` — checked конверсии в фиксированную ширину (overflow → None).

- `BigFloat @to_bigint() -> BigInt` — truncation: mant × 2^{exp}, отбрасывая дробную часть.
  ```
  // Если exp >= 0: mant × 2^{exp} (BigInt << exp)
  // Если exp < 0:  TRUNC-К-НУЛЮ — сдвигать |mant| >> |exp| и вернуть знак.
  //   ГОЛЫЙ сдвиг знакового вправо — ЛОВУШКА (фикс ревью 2026-07-30): арифметический
  //   сдвиг = floor (-5 >> 1 = -3), а trunc(-2.5) = -2. Семантика BigInt `>>` на
  //   отрицательных (floor vs trunc) — ПИН Ф.0; sign-magnitude 235 естественно даёт
  //   «сдвиг модуля» = trunc, но зафиксировать пин-тестом, не предполагать.
  ```
  (Не round, а trunc — паритет Go `Float.Int(z)` / rug `to_integer`. Для округления → @round.)

- `BigFloat.is_integer() -> bool` — проверка без округления:
  ```
  exp >= 0 || trailing_zero_bits(mant) >= |exp|
  // (НЕ материализовать маску (1 << |exp|) - 1 — при большом |exp| это гигантский BigInt
  //  ради одного @bitand; имя метода по D46-амендменту — @bitand, не @bit_and)
  ```

- Конструкторы: `BigFloat.zero()` → `{ mant: BigInt.zero(), exp: 0 }`,
  `BigFloat.one()` → `{ mant: BigInt.one(), exp: 0 }`.
  (`two()` снят — ревью 2026-07-30: единственный потребитель был `sqrt`-÷2, а деление на
  степень двойки это `exp - 1`; полный `@div` на неё — расточительность.)

- `str @to_bigfloat(ctx PrecisionContext) -> Result[BigFloat, ParseBigFloatError]` — парсинг десятичной строки в BigFloat:
  ```
  // Парсим как BigDecimal (236 §2.9): mant_bd, scale
  // Если scale == 0: mant_bf = mant_bd, exp_bf = 0 (целое — точно)
  // Если scale > 0:
  //   value = mant_bd × 10^{-scale}
  //         = mant_bd × 2^{-scale} × 5^{-scale}
  //   k = ctx.prec + GUARD               // битов запаса для однократного округления
  //   dividend = mant_bd × 2^{k}         // BigInt << k
  //   (quot, rem) = dividend.div_rem(5^{scale})
  //   sticky = rem != 0
  //   // quot × 2^{-(k + scale)} ≈ value
  //   (mant_bf, bits_lost) = round_to_precision(quot, ctx.prec, ctx.rm, sign, sticky)
  //   exp_bf = -scale - k + bits_lost
  // ERR: тот же ParseBigDecimalError, переименованный в ParseBigFloatError
  ```
  **⚠** 5^{scale} — BigInt-степень (до ~2.3M бит при scale=10^6). V1: корректность > скорость.

- `BigFloat @to_str(frac_digits int = 6) -> str` — форматирование как десятичной строки:
  ```
  // Алгоритм: конвертируем BigFloat → BigDecimal → строку (переиспользует to_str BigDecimal)
  // BigFloat → BigDecimal:
  //   Если exp >= 0: mant × 2^{exp} — точное целое → BigDecimal(mant × 2^{exp}, 0)
  //   Если exp < 0:  mant / 2^{|exp|}
  //     = mant × 5^{|exp|} / 10^{|exp|}
  //     = BigDecimal(mant × 5^{|exp|}, |exp|)
  // frac_digits — число десятичных цифр после запятой (default 6, как printf "%.6g")
  // ФИНАЛ (фикс ревью 2026-07-30 — шаг отсутствовал): полученный BigDecimal ОБЯЗАН быть
  // приведён к frac_digits ДЕСЯТИЧНЫМ округлением (@rescale(frac_digits, HalfEven), 236)
  // ПЕРЕД печатью — иначе точная форма x×2^{-52} напечатает 52+ цифр (десятичное
  // разложение двоичной дроби конечно, но длинно).
  ```
  **Performance:** `5^{|exp|}` при |exp|=1023 (f64-диапазон) — BigInt с ~2400 битами, приемлемо. При |exp| > 10^6 — документировано медленно.

- `BigFloat @to_bigdecimal() -> BigDecimal` — см. выше (mant × 5^{|exp|}).

- `BigDecimal @to_bigfloat(ctx PrecisionContext) -> BigFloat` — обратная конверсия (округляет десятичную дробь до двоичной; идентично str-алгоритму):
  ```
  // value = bd.mant × 10^{-scale}
  // k = ctx.prec + GUARD
  // (quot, rem) = (bd.mant × 2^{k}).div_rem(5^{scale})
  // (mant_bf, bits_lost) = round_to_precision(quot, ctx.prec, ctx.rm, sign, rem != 0)
  // exp_bf = -scale - k + bits_lost
  ```

- `BigFloat @round(prec int, rm RoundingMode) -> BigFloat` — округление значащих бит (`prec` в битах, не путать с `frac_digits` в `to_str`):
  ```
  // Работает как round_to_precision, но на самом значении, не на wide-промежуточном результате.
  // 1. Нормализовать self.
  // 2. (mant, bits_lost) = round_to_precision(|self.mant|, prec, rm, self.mant.sign, sticky=false)
  //    exp = self.exp + bits_lost
  // 3. → BigFloat { mant: apply_sign(mant, sign), exp }
  // При prec >= bit_length(self.mant) → self (no-op).
  ```

**⚠ Конверсии str↔BigFloat — самый дорогой путь.** `str → BigFloat` требует BigInt-умножения на степень 5 (до тысяч цифр). `BigFloat → str` требует BigInt-умножения на степень 5 и BigInt-деления. BigFloat → BigDecimal → str дешевле (BigDecimal.to_str уже реализован), но тоже O(n²). Для V1 этого достаточно; V2 — Dragon4/Grisu3.

### 2.6. Операторный desugar

**Desugar:** `+`/`-`/`*` → `@plus(ctx)/@minus(ctx)/@times(ctx)`. **НО** операторный desugar на Nova не передаёт `ctx` неявно. Стало быть, `a + b` для BigFloat **невозможен** как дефолтный desugar — операторы бесконтекстны.

**Решение V1:** операторный desugar НЕ реализован. Все операции — явные методы:
```
a.@plus(b, ctx)
a.@minus(b, ctx)
a.@times(b, ctx)
a.@div(b, ctx)
a.@sqrt(ctx)
```

**Обоснование:** (1) Тот же подход, что у `BigDecimal./` (не оператор); (2) IEEE 754 подразумевает rounding на КАЖДОЙ операции — неявный thread-local или default контекст нарушает referential transparency; (3) прецедент в экосистеме: Java `BigDecimal.add(MathContext)`, Rust `BigDecimal.with_context()`. Если в будущем появится механизм default-контекста (scope-local или handler) — операторы можно до-десугарить.

`-x` → `@neg()` допустим (без контекста — знак-флип). Оператора `|x|` в Nova НЕТ — абсолютное значение только методом `@abs()`.

### 2.7. Литералов, коэрций и implicit widening НЕТ

Никакого `12.345bf`-синтаксиса или неявного `int → BigFloat`. Только явные `@to_bigfloat(ctx)`. Обоснование: то же, что в BigDecimal (D429 — аллокация вне zero-cost-полосы).

### 2.8. Дом — в репе `nova-bigint`

BigFloat — в той же репе `nova-bigint` (решение владельца: BigInt, BigDecimal, BigRat, BigFloat вместе, как Go `math/big`). Подпакет `bigfloat` рядом с `bigint`/`bigdecimal`/`bigrat`. Тесты — `bigfloat_test.nv` рядом.

### 2.9. Формат строки для `str @to_bigfloat`

Возврат — `Result[BigFloat, ParseBigFloatError]`. Входной формат — тот же, что у BigDecimal (236 §2.9), плюс:

```
bigfloat-str    := [sign] (int-part ['.' [frac-part]] | '.' frac-part) [exp-part]
bigfloat-binary := [sign] '0b' bin-digits ['.' bin-frac] 'p' [sign] digits
// 0b101.01p3 = 5.25 × 2^3 = 42.0
// РЕКОМЕНДАЦИЯ РЕВЬЮ (к решению Ф.0(2)): суффикс 'bf' УБРАН — вторая дверь без
// информации (функция и так вызвана как to_bigfloat, а двоичный формат однозначно
// различим по префиксу '0b'). Прежняя редакция вдобавок противоречила себе:
// грамматика допускала 'bf' на десятичной строке, а комментарий приписывал
// суффиксу «двоичность».
```

**Примеры (десятичный ввод):**
| Вход | Результат (схематично) | Пояснение |
|---|---|---|
| `"12.345"` | mant=12345·2^{k}/5^{3} → round, exp=-3-k+bits | scale=3; mant_bd=12345 |
| `"1e-3"` | mant=1·2^{k}/5^{3} → round, exp=-3-k+bits | scale=3; mant_bd=1 |
| `"3.14159265358979323846"` | mant_bd=big, scale=20, → BigFloat с prec=128 | π с произвольной точностью |
| `"123"` (scale=0) | mant=123, exp=0 | целое — точный |

**Примеры (двоичный ввод):**
| Вход | Результат |
|---|---|
| `"0b101.01p0"` | mant=0b10101, exp=-2 |
| `"0b1p1"` | mant=1, exp=1 = 2.0 |
| `"-0b1.1p-1"` | mant=-0b11, exp=-2 = -0.75 |

## 3. Фазы

- **Ф.0 Разведка/дизайн-фиксация (короткая, после закрытия Plan 235+236):** (1) пин-тест `str @to_bigfloat(parseBigDecimal → BigDecimal → BigFloat)` — сколько времени занимает BigInt-умножение на 5^{n} при n=100/1000/10000; (2) решение владельца: суффикс `bf` для бинарного формата или нет (рекомендация ревью: НЕТ — 0b-префикс достаточен, суффикс = вторая дверь); (3) API-ревью владельцем; (4) пин `@plus`/`@minus`/`@times` — подтвердить отсутствие desugar и утвердить имена методов; (5) пин-тест знаковой конвенции BigInt `@div_rem` (235: trunc-к-нулю, rem знака делимого) — на ней держатся все разделы; (6) семантика BigInt `>>` на ОТРИЦАТЕЛЬНЫХ (floor или trunc) — критична для `@to_bigint`/`normalize`; sign-magnitude 235 естественно даёт «сдвиг модуля» = trunc, зафиксировать пин-тестом; (7) стоп-критерий `sqrt` — воспроизвести осцилляцию Ньютона на векторе (prev/cap из §2.3 обязателен, не «до строгого равенства»).

- **Ф.1 Представление + нормализация + конверсии (sonnet):** `type BigFloat value {…}`, `type PrecisionContext`, `type RoundingMode` (если не вынесен из 236); `int @to_bigfloat`/`f64 @to_bigfloat`/`str @to_bigfloat`; `to_str`/`normalize`; `BigFloat ↔ BigDecimal` (BigFloat→BigDecimal: ×5^{|exp|}; BigDecimal→BigFloat: ×2^{k}÷5^{scale}); `@to_f64`/`@to_int`/`@to_i64`/`@to_u64`/`@to_bigint`/`@sign`/`is_*`.

- **Ф.2 Сложение, вычитание, умножение (sonnet):** выравнивание порядков (BigInt `<< k`), BigInt `@plus/@minus/@times`, round_to_precision с GUARD=3; все комбинации знаков. Тесты: PRNG-identity против f64 на представимых значениях; канонические вектора (разность порядков от -100 до +100, одинаковые знаки/разные).

- **Ф.3 Деление (sonnet):** расширение делимого на `ctx.prec + GUARD` битов, BigInt `@div_rem`, sticky, округление. Тесты: 1/3, 1/7, 1/10, π/4 при prec=24/53/128; матрица 7 rounding modes.

- **Ф.4 sqrt (sonnet):** Ньютон, выбор начального приближения через `@to_f64`, сходимость. Тесты: sqrt(2), sqrt(3), sqrt(0), sqrt(1), sqrt(very_large).

- **Ф.5 Тесты (sonnet):** PRNG-identity против MPFR-эталона (через C-скрипт, генерирующий пары (BigFloat-str, prec, expected-str), как `spec_tests/plan152_4` делает conformance из UCD); edge-вектора: 0, бесконечно малые (subnormal-range), деление на ноль → panic; `to_str` round-trip (`x.to_str(n).to_bigfloat(ctx).to_str(n) ≈ x.to_str(n)`); `f64` round-trip; `BigDecimal ↔ BigFloat` round-trip; `@equal` нормализация.

- **Ф.6 Закрытие:** doc-комменты, STATUS-строка, запись в simplifications при упрощениях.

## 4. Гейты

Таргетно: `nova test` репы `nova-bigint` зелёный; `--strict-effects`; линт чистый. Авторитетный (интегратор): conformance-CU не задет (внешняя репа). BigDecimal-зависимость: если BigDecimal ещё не влит в nova-bigint к моменту Ф.1 — временно замокать `BigDecimal` как стуб (только str-конверсии без round-trip, реализовать тупой `str↔BigFloat` напрямую через `5^{k}` BigInt-умножение без BigDecimal-посредника).

## 5. Риски

| Риск | Митигация |
|---|---|
| `str → BigFloat`: 5^{scale} на scale=10^6 — BigInt из ~2.3M бит (десятичная строка с 10^6 цифр) — O(n²) умножение упадёт по памяти | V1: документировать, что scale > 10^5 медленно. Для таких чисел рекомендовать двоичный ввод `0b...p...bf` или BigDecimal + to_bigfloat. |
| `f64 @to_bigfloat`: субнормальные числа (±5e-324) | Поддержать: mant_bits=0, exp=-1022-52. BigInt представление с exp=-1074 — корректно, разрядности хватит. Тест: `f64.MIN_POSITIVE.@to_bigfloat().@to_f64() == Some(f64.MIN_POSITIVE)`. |
| Ньютон для sqrt: начальное приближение из f64 может дать 0 для subnormal | Проверка: если f64-приближение 0 — начать с `BigFloat { mant: 1, exp: self.exp / 2 }`. |
| Деление: sticky вычисляется по последнему rem — но при carry после округления sticky может измениться | Тот же подход, что в BigDecimal 236 §2.3: sticky фиксируется ДО округления (на rem от div_rem). Carry меняет только последнюю цифру/бит, sticky в отброшенном хвосте не меняется. |
| Guard bits: сколько достаточно для однократного правильного округления? | IEEE 754 требует 3 бита (guard + round + sticky). Для произвольной точности с последующим round_to_precision — GUARD=3 достаточно (доказано MPFR, Steele & White 1990). |
| scale overflow при конверсии BigDecimal→BigFloat: bd.scale может быть int::MAX | Паритет BigDecimal-риска: panic при overflow (паритет int-арифметики Nova). |
| `@compare`: сдвиг выравнивания | СНЯТ ревью 2026-07-30: msb-fast-path (§2.4) решает за O(1) все случаи разных msb; при равных msb сдвиг ограничен разницей bit_length мантисс — не взрывается. Прежний MAX_SHIFT-хак был вдобавок НЕВЕРЕН (сравнивал по exp, а не по msb) |
| Сложение при огромной разнице порядков: `a_abs << (diff + GUARD)` при diff=10⁶ — мегабитный BigInt (в @compare устранено, в @plus — нет) | V1: документировано (§2.3); V2 — MPFR-приём: при diff > prec + GUARD меньший операнд → один sticky-бит без сдвига |
| Парсинг `0b...p...`: нужно отличать от обычной десятичной строки | Префикс `0b` однозначен (не может начать десятичное число). Обратная совместимость: строки без префикса — десятичный парсинг. |

## 6. Вне объёма (V2+ по отдельным решениям)

| Фича | Мотивация |
|---|---|
| Трансцендентные функции (`exp`, `ln`, `sin`, `cos`, `tan`, `pow`) | Стандарт MPFR. Каждая — свой план с Agner Fog-алгоритмами и таблицами констант |
| FMA (`@fma(a, b, c) = a*b + c` с одним округлением) | IEEE 754-2019, снижает double-rounding в цепочках |
| Цепные операции с auto-precision | `a.plus(b, ctx)` вместо `with ctx { a + b }` или implicit scope ctx |
| Интеграция с generic арифметикой (D310) | `fn[T Float] T @to_bigfloat(ctx) -> BigFloat` |
| Неявные коэрции и литералы | `12.345bf` / разумный default-context |
| Dragon4/Grisu3 для `to_str` | Быстрое и точное binary↔decimal для больших экспонент |
| BigInt-оптимизации степеней 5 для str-конверсий | Предвычисление `5^{2^k}` таблицы |
| `@to_f128` (binary128 quadruple precision) | Если появится тип f128 |
| Константы: `BigFloat.pi(prec)`, `BigFloat.e(prec)` | π/e с произвольной точностью через алгоритмы Гаусса-Лежандра / AGM |
| `to_str_radix(radix, prec)` и `to_str_binary` | Go `Text(fmt, prec)`, rug `to_string_radix` |
| `@exp` / `@get_mantissa` — raw access | Go `MantExp`, rug `get_exp`/`get_significand` |
| `@is_inf` / `@is_finite` / `@is_normal` | Специальные значения (V1 нет Inf) |
| `@min_prec` — минимальная точность для точного представления | Go `MinPrec`, rug `prec` отражает stored prec |

## Связи

[Plan 235](235-bigint.md) (зависимость — BigInt фундамент для мантиссы) · [Plan 236](236-bigdecimal.md) (BigDecimal — конверсии str↔BigFloat) · D423 (trap-политика — panic на div-by-zero, Inf/NaN) · D429 (почему коэрций нет: аллокация вне zero-cost-полосы) · IEEE 754-2019 §4.3 (rounding modes) · MPFR (эталон произвольной точности, алгоритмы) · Go `math/big.Float` (модель подпакетов, API — `Int64/Uint64/Int/Float64/Sign/IsInt/MantExp/Text`) · Rust `rug::Float` (wraps MPFR, эталон API — `to_i32/to_f64/is_integer/to_integer/to_string_radix`) · Steele & White «How to Print Floating-Point Numbers Accurately» (binary↔decimal) · `std/src/math/int128.nv` (фиксированная ширина) · D310 (type-set bounds — интеграция в generic-арифметику V2).
