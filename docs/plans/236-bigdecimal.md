<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 236 — `std.math.bigdecimal`: десятичная произвольной точности поверх BigInt

**Статус:** 📋 ОФОРМЛЕН 2026-07-29. Реализация НЕ начата. **Приоритет:** P3 (не блокирует v0.1; стек ПОСЛЕ [235](235-bigint.md) — BigInt-фундамент первичен).

**Зависимость:** блокирован планом 235 (BigInt). Все арифметические операции BigDecimal сводятся к BigInt; BigDecimal не может быть реализован или протестирован до готового BigInt.

## 0. Мотив

BigDecimal — десятичное число произвольной точности: `mantissa × 10^{-scale}`. Это единственный корректный тип для денежных расчётов, бухгалтерии, финансовых вычислений и вообще любых величин, где `0.1 + 0.2 ≠ 0.30000000000000004` недопустимо (IEEE 754 binary64 round-off).

BigDecimal строится **исключительно поверх BigInt** (Plan 235) — так же как Java `BigDecimal` поверх `BigInteger` и Python `Decimal` поверх произвольной точности. Собственных лимбов не имеет; единственная добавленная сложность — масштаб (десятичная экспонента) и MathContext (точность + округление).

Nova-специфика: BigDecimal — value-record (стек): `type BigDecimal value { mant BigInt, scale int }`. BigInt — heap-record (Vec), но копия value-record'а копирует только указатель на BigInt, не лимбы. Операторный desugar через `@plus`/`@minus`/`@times` на value-record — прецедент есть (`str @plus`). `/` не десугарится (требует MathContext).

## 1. Объём V1 — ТОЛЬКО точная десятичная арифметика

**V1 = корректность, читаемость, покрытие; никаких оптимизаций.** BigDecimal-V1 — обёртка над BigInt, реализующая:

- представление + нормализация
- конверсии `int → BigDecimal`, `i128 → BigDecimal`, `str → BigDecimal`, `BigDecimal → str`
- `@plus`/`@minus`/`@times`/`@neg`/`@abs`
- `@div` с MathContext (точность + rounding mode; только метод, не оператор `/`)
- `@compare`/`@equal`
- `@round` (precision-based, значащие цифры) и `@scale` (scale-based, десятичные знаки)

V1 **не** включает: цепные операции с автоматической точностью; `pow`/`sqrt`; интеграцию с generic-числовыми type-sets (D310); неявные коэрсии.

## 2. Дизайн (V1)

### 2.1. Представление

```nova
type RoundingMode enum
    HALF_EVEN   // banker's rounding (Java default)
    HALF_UP     // школьное 0.5 → +1
    HALF_DOWN   // 0.5 → 0
    DOWN        // truncation (Java FLOOR аналог) — к нулю
    UP          // от нуля (CEILING для положительных)
    CEILING     // к +∞
    FLOOR       // к -∞

type MathContext {
    precision u64,     // количество ЗНАЧАЩИХ цифр мантиссы (не знаков после запятой)
    rm RoundingMode,
}

type BigDecimal value {
    mant BigInt,
    scale int,         // значение = mant × 10^{-scale}; может быть < 0
}
```

**Value-record:** BigDecimal на стеке (копия = 8 байт pointer + 8 байт int). BigInt-мандиса — heap-record, но разделяется между копиями (копируется указатель). Иммутабельность BigInt гарантирует отсутствие alias-мутаций.

Инварианты:
- `mant` нормализована (BigInt-инварианты: без ведущих нулей; ноль = `Sign.Zero` + пустые limbs).
- `scale` — любое значение типа `int` (знаковое, 64-битное). Отрицательный scale = целое с неявными конечными нулями: `BigDecimal { mant: 123, scale: -2 }` = `123 × 10^{2} = 12300`.
- MAX_SCALE не фиксирован (bound = `int::MAX`, как Rust). В V1 panic не предусмотрен — scale растёт, пока хватает памяти под BigInt ×10^k.

### 2.2. Арифметика — вся через BigInt

**Сложение / вычитание:**
```
scale = max(a.scale, b.scale)
a_mant = a.mant × 10^{scale - a.scale}
b_mant = b.mant × 10^{scale - b.scale}
mant = a_mant ± b_mant  (BigInt @plus/@minus)
→ BigDecimal { mant, scale }  // без decimal-normalize (lazy)
```

Умножение одной мантиссы на `10^k` — BigInt `@times` на степень десятки (которая сама BigInt, вычисляется как `10.pow(k)` через BigInt умножение; кэширование малых степеней — V2).

**Умножение:**
```
mant = a.mant × b.mant  (BigInt @times)
scale = a.scale + b.scale
→ BigDecimal { mant, scale }  // без decimal-normalize (lazy)
```

**Деление (ключевая сложность — как у BigInt Кнут D):**
```
// Вычислить mant = a / b с точностью p значащих цифр (p = mc.precision)
// a / b = (a.mant × 10^{a.scale}) / (b.mant × 10^{b.scale})
//        = (a.mant × 10^{a.scale - b.scale}) / b.mant

// Расширяем делимое на p + 2 цифры — p цифр результата + 1 для округления
// + 1 запас на возможный carry после округления (9.95 → 10.0 при p=2).
scale_diff = a.scale - b.scale
extended_precision = p + 2 + max(0, -scale_diff)
extended = a.mant × 10^{extended_precision}

(quot, rem) = BigInt @div_rem(extended, b.mant)

// quot имеет ≤ p + 2 значащих цифр.
// Перед округлением отбрасываем младшую цифру → p + 1 остаётся.
(quot, round_digit) = chop_last_digit(quot)

// Десятичное округление последней цифры (0..9) по mc.rm:
rounded = apply_rounding(quot, round_digit, mc.rm)

// Если carry дал p+1 цифр (99→100), сдвигаем scale:
if rounded.digits() > p:
    rounded /= 10
    result_scale = p - 1
else:
    result_scale = p

→ BigDecimal { mant: rounded, scale: result_scale }
```

### 2.3. Округление — `MathContext.apply(mant, target_precision, rm)`

Отбрасывание младших цифр мантиссы до `target_precision`:
```
factor = 10^{mant.digits() - target_precision}
(quot, rem) = mant.div_rem(factor)
half = factor / 2
carry = match rm:
    HALF_EVEN => rem > half || (rem == half && quot.is_odd())
    HALF_UP   => rem >= half
    HALF_DOWN => rem > half
    DOWN      => false  // truncate
    UP        => rem != 0
    CEILING   => quot.sign == Pos && rem != 0
    FLOOR     => quot.sign == Neg && rem != 0
rounded = quot + (carry ? 1 : 0)
→ rounded
```

### 2.4. Нормализация — `BigDecimal.normalize()`

Удаление конечных десятичных нулей мантиссы (уменьшение `scale` без изменения значения).
Работает одинаково при любом знаке scale:

```
loop:
    if mant.is_zero() → return BigDecimal(mant=Zero, scale=0)
    (mant, rem) = mant.div_rem(10)  // BigInt @div_rem
    if rem != 0 → break  // последняя цифра не ноль
    scale -= 1
// повтор
```

**⚠ O(n²):** BigInt `div_rem(10)` — полный проход по лимбам; нормализация числа с 500 конечными нулями стоит 500 полных делений. V1: корректность > скорость; V2 требует trailing-zero-count через степени 10 (бинарный поиск).

**Lazy normalize:** Нормализация НЕ вызывается в конструкторах и операциях (+/-/×). Только в `@equal`, `@hash` и при явном вызове пользователем. Паритет с Rust `bigdecimal`: после цепочки `a + b + c` значение сохраняет trailing zeros; первый `@equal`/`@hash` нормализует.

### 2.5. Конверсии

- `fn[T Ints] T @to_bigdecimal() -> BigDecimal` — `BigInt::from(T)`, `scale = 0`.
- `i128 @to_bigdecimal() -> BigDecimal` — явная перегрузка (i128 не член Ints; Ints может отсутствовать к моменту V1 — тогда все перегрузки явные).
- `str @to_bigdecimal() -> Option[BigDecimal]` — см. 2.9 «Формат строки».
- `BigDecimal @to_str(scale_pad: int? = None) -> str`:
  ```
  if scale >= 0:
      (int_part, frac) = mant.div_rem(10^{scale})
      // frac дополняется слева нулями до scale цифр
      int_part @to_str + '.' + pad_left(frac @to_str, scale, '0')
  if scale < 0:
      // целое с неявными нулями: mant × 10^{-scale}
      (mant × 10^{-scale}) @to_str  // без точки
  ```
  Параметр `scale_pad` — минимальное число цифр после запятой (дополнение справа нулями).
- `BigDecimal @to_int() -> Option[int]` — fits-проверка (BigInt @to_int).
- `BigDecimal @to_i128() -> Option[i128]` — явная перегрузка (i128 не Ints).
- `BigDecimal @round(ctx: MathContext) -> BigDecimal` — округление до `ctx.precision` значащих цифр (precision-based, см. 2.3).
- `BigDecimal @scale(target: int, rm: RoundingMode) -> BigDecimal` — округление до `target` десятичных знаков (scale-based, установка свойства scale). Аналог Java `setScale(int, RoundingMode)`. Если `target > scale` — мантисса дополняется нулями (без округления); если `target < scale` — округление по `rm`. `target < 0` — округление до `10^{|target|}` (аналог Java `setScale(-2)` = to hundreds).

### 2.6. Операторный desugar и равенство

**Desugar:** `+`/`-`/`*` → `@plus/@minus/@times`. `/` **НЕ десугарится** — `@div(b, mc)` требует явного `MathContext` (деление BigDecimal неоднозначно без точности). Оператор `a / b` в V1 не поддерживается; только `a.@div(b, mc)`.

**Равенство:** BigDecimal — value-record. В Nova `==` на value-record **структурное** по умолчанию (D328): field-by-field (BigInt-поля через BigInt.@equal, int-поля через `==`). `#impl(Equal)` не требуется для `==`, но нужен для `#impl(Compare)`, `@hash` и использования в `HashMap`.

**Lazy normalize + @equal:** Поскольку normalize не вызывается в операциях, два значения с одинаковым числовым значением могут иметь разное mant+scale (`1.0 = {10, 1}`, `1 = {1, 0}`). Структурное `==` на value-record дало бы Java-gotcha. Решение:

- `@equal` нормализует ОБА операнда перед memberwise-сравнением (паритет Rust `bigdecimal`).
- `@hash` нормализует перед хэшированием.
- `@compare` нормализует, затем лексикографически по (scale, mant) или через BigInt.

**⚠** Первый `@equal`/`@hash` на ненормализованном значении платит O(n²). Последующие вызовы — O(1), если значение уже нормализовано. Для V1 этого достаточно; V2 — кэш нормализованной формы в value-record или lazy-normalize-on-construction через trailing-zero-count.

**Прецедент:** `str @plus` (02-types.md ~9075) — value-record с heap-данными, `+` работает. Ф.0 подтверждает desugar для BigDecimal.

### 2.7. Литералов и коэрций НЕТ

Никакого неявного `int → BigDecimal` (аллокация ≠ zero-cost — вне полос D429 #coerce). Только явные `to_bigdecimal`-вызовы. Литеральная форма `12.345bd`-стиля — не в V1.

### 2.8. Дом — в репе `nova-bigint`

BigDecimal НЕ отдельная репа — живёт в той же репе `nova-bigint` (решение владельца: BigInt, BigDecimal, BigRat, BigFloat — все в одном внешнем пакете, как Go `math/big`). Подпакет `bigdecimal` рядом с `bigint`/`bigrat`/`bigfloat`. Тесты — `bigdecimal_test.nv` рядом.

### 2.9. Формат строки для `str @to_bigdecimal`

Спецификация входного формата (не хуже Rust `bigdecimal`, но без Unicode-digit-сюрпризов Java):

```
bigdecimal-str := [sign] (int-part ['.' [frac-part]] | '.' frac-part) [exp-part]
sign           := '+' | '-'
int-part       := digit (digit | '_')*
frac-part      := digit (digit | '_')*
exp-part       := ('e' | 'E') [sign] digits
digits         := digit (digit | '_')*
digit          := '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9'
```

**Правила:**
- Символ `_` удаляется из digit-последовательностей ДО вычисления scale (как в Rust/Python 3.6+). `1_000.5_00` → `1000.500`.
- Ровно одна точка; второе вхождение `'.'` → `None`. Ровно одна экспонента; второе `'e'`/`'E'` → `None`.
- Пустой int-part или frac-part после удаления `_` → `None` (кроме случая `'.'` с обеих сторон пусто → тоже `None`).
- Экспонента без digits после sign (или без sign и без digits) → `None`.
- Scale = `len(frac_part_cleaned) - exp_value`. Scale может быть отрицательным (экспонента больше числа дробных цифр).
- Обработка только ASCII `'0'..'9'`. Не-ASCII цифры (арабские, деванагари) → `None`.

**Примеры:**
| Вход | Результат | Почему |
|---|---|---|
| `"12.345"` | mant=12345, scale=3 | 3 цифры после точки |
| `"12.340"` | mant=12340, scale=3 | (normalize → mant=1234, scale=2) |
| `".5"` | mant=5, scale=1 | пустой int-part |
| `"5."` | mant=5, scale=0 | пустой frac-part |
| `"1e-3"` | mant=1, scale=3 | экспонента -3: 0 - (-3) = 3 |
| `"1.5E+2"` | mant=15, scale=-1 | frac=1 цифра, exp=2: 1-2=-1 |
| `"1_000.50"` | mant=100050, scale=2 | `_` удалён |
| `"123.45.6"` | `None` | две точки |
| `"1e"` | `None` | пустая экспонента |
| `""` | `None` | пустая строка |
| `"hello"` | `None` | не-цифры |

## 3. Фазы

- **Ф.0 Разведка/дизайн-фиксация (короткая, после закрытия Plan 235):** (1) фикстура операторного desugar на value-record с `@plus/@compare` для BigDecimal — подтвердить, что `+`/`<` работают; (2) выбор `MathContext`-формата; (3) API-ревью владельцем.

- **Ф.1 Представление + нормализация + конверсии (sonnet):** `type BigDecimal value {…}`, `type MathContext`, `type RoundingMode`; `T @to_bigdecimal`/`i128 @to_bigdecimal`/`str @to_bigdecimal`; `to_str`/`normalize`.

- **Ф.2 Сложение, вычитание, умножение (sonnet):** выравнивание scale (BigInt `×10^k`), BigInt `@plus/@minus/@times`; все комбинации знаков (Sign-таблица); `@neg`, `@abs`, `@compare`, `@equal`. Тесты: PRNG-identity против `f64` на представимых значениях (без переполнения double) + канонические вектора (сумма/разность с разными scale, в т.ч. отрицательными).

- **Ф.3 Деление (sonnet, самый рискованный кусок):** расширение делимого на `p+2` цифры (p результат + 1 округление + 1 запас на carry), BigInt `@div_rem`, отбрасывание младшей цифры (`chop_last_digit`), округление по RoundingMode, коррекция scale при carry. Матрица 7 режимов × typical edge (0.5, 1.5, 2.5, -0.5, →∞/→0/→-∞). Проверка: `a / b * b` ≈ `a` в пределах точности.

- **Ф.4 MathContext + явное `@round` / `@scale` (sonnet):** precision-based `@round(ctx)`, scale-based `@scale(target, rm)`, цепные операции с контекстом.

- **Ф.5 Тесты (sonnet пишет карту+тонкие, haiku — вектора по литеральному образцу):** PRNG-identity против Java `BigDecimal`-эталона (по значениям, не по коду); edge-вектора: деление на ноль → panic (паритет int), `HALF_EVEN` banker's rounding (`2.5 → 2`, `3.5 → 4`), крайние scale (±10^5, 0), 0 в разных scale; отрицательный scale после `1.5E+2`; `to_str` round-trip (`x.to_str().to_bigdecimal() == x`); `@equal` compareTo-семантика (`1.0 == 1`); `@scale` с отрицательным target.

- **Ф.6 Закрытие:** doc-комменты, STATUS-строка, запись в simplifications при упрощениях.

## 4. Гейты

Таргетно: `nova test` репы `nova-bigint` зелёный; `--strict-effects`; линт чистый. Авторитетный (интегратор): conformance-CU не задет (внешняя репа).

## 5. Риски

| Риск | Митигация |
|---|---|---|
| Нормализация: `div_rem(10)` в цикле — O(n²) при большом числе конечных нулей | Вызывается только в `@equal`, `@hash` и явном `normalize()`. V1: корректность > скорость. Документировать: normalize дорог для чисел с >10⁴ trailing zeros |
| Деление: quot-догадка BigInt div_rem (Кнут D) может давать лишнюю цифру при малых знаменателях | Ф.3 явно проверяет `quot.digits()` и корректирует scale |
| Округление: `HALF_EVEN` требует проверки чётности — BigInt `@is_even()` за O(1) | `is_even` на BigInt тривиален |
| scale overflow: `add(a,b)` → `|a.scale ± b.scale|` может превысить `int::MAX` | Паритет Rust: scale = int (64-bit), overflow физически невозможен для представимых чисел (2⁶³ знака > атомов во вселенной). Документировать: не паниковать, обрезать до `int::MAX`? или panic? Рекомендация: panic по паритету `int` overflow в Nova |
| Операторы на value-record не десугарятся (`@plus`/`@times`) | Ф.0-пин; fallback = методы без операторов (`a.plus(b)` вместо `a + b`) |
| `Ints` type-set может быть не готов к V1 Nova → `fn[T Ints] T @to_bigdecimal` не скомпилируется | Явные перегрузки `i8 @to_bigdecimal`, … `u64 @to_bigdecimal` как fallback; generic — если доступен |
| `@scale(target, rm)` с отрицательным target: округление до `10^{|target|}` | Документировано в API. `BigDecimal(1234, 0).@scale(-2, DOWN)` = `BigDecimal(12, -2)` (= 1200). Тесты: Ф.5 |

## 6. Вне объёма (V2+ по отдельным решениям)

Оператор `/` (с thread-local или default `MathContext`); `pow`/`sqrt`; мутабельные `@add_assign`/`@sub_assign`/`@mul_assign` для GC-шума в циклах; цепные операции с автоматическим переносом точности (`a.plus(b, mc)` — однооконная форма, не `mc` как receiver); интеграция с generic-числовыми type-sets (D310); неявные коэрсии и литералы; кэш малых степеней 10; Toom-Cook для умножения; `remainder`/`divideToIntegralValue` (Java-style).

## Связи

[Plan 235](235-bigint.md) (зависимость — BigInt фундамент для мантиссы) · D423 (trap-политика ядра — BigDecimal наследует panic на div-by-zero) · D429 (почему коэрций нет: аллокация вне zero-cost-полосы) · Java `java.math.BigDecimal` / Python `decimal.Decimal` / Go `math/big` (эталоны API) · `std/src/math/int128.nv` (фиксированная ширина) · D310 (type-set bounds — интеграция в generic-арифметику V2).
