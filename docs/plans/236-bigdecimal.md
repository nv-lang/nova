<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 236 — `bigdecimal` (пакет nova-bignum): десятичная произвольной точности поверх BigInt

**Статус:** ✅ ЗАКРЫТ 2026-07-31 — V1 сдан (окно sonnet, приёмка интегратора: check 5/0, test 4/0/1, запушено на 3 ремоута). Три пина Ф.0 проверены фактом; 3 дефекта компилятора заведены (№170-№172), порядок атрибутов — backlog.

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
// D406-канон: варианты через `|`, PascalCase (как `Sign enum Neg | Zero | Pos` в 235;
// SCREAMING_CASE — Java-стиль, у нас так пишутся только const).
type RoundingMode enum HalfEven | HalfUp | HalfDown | Down | Up | Ceiling | Floor
// HalfEven — banker's (Java default) · HalfUp — школьное 0.5→+1 · HalfDown — 0.5→0
// Down — к нулю (truncation) · Up — от нуля · Ceiling — к +∞ · Floor — к -∞

type MathContext value {   // value: дешёвая пара, копия честная
    precision int,   // ЗНАЧАЩИХ цифр мантиссы (не знаков после запятой); ИНВАРИАНТ ≥ 1.
                     // Java precision=0 «unlimited» НЕ поддерживаем в V1: неограниченное
                     // 1/3 не терминируется; конструктор с 0 — panic/requires.
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

**Деление (ключевая сложность):**
```
// ТРЕБОВАНИЕ: !b.is_zero() — деление на ноль паникует (паритет int; норма ДИЗАЙНА, не только тест).
// a/b = (a.mant / b.mant) × 10^{-(a.scale - b.scale)}.
// Значащих цифр у квотиента ≈ digits(a.mant) - digits(b.mant) + k (±1) при расширении
// делимого на 10^k — k ОБЯЗАН учитывать длины мантисс.
// (Фикс ревью 2026-07-30: прежняя редакция расширяла на p+2 БЕЗ учёта digits и клала
//  result_scale = p — неверно уже на 100/8 при p=4: quot получал 7 цифр, «сдвиг на одну»
//  не спасал; плюс округление по одной отброшенной цифре ТЕРЯЛО sticky-хвост от rem —
//  2.5000001 под HalfEven округлялся бы как ровно 2.5.)

sign = знак результата (произведение знаков); далее работаем на |mant| —
       BigInt @div_rem (235) trunc-к-нулю, rem знака ДЕЛИМОГО: сравнения с half
       на отрицательных ломаются, знак навешивается в конце
da = a.mant.digits(); db = b.mant.digits()
k = max(0, p + 2 - da + db)                    // квотиент получит >= p+1 значащих цифр
(quot, rem) = (|a.mant| × 10^k).div_rem(|b.mant|)
sticky = rem != 0                              // ненулевой хвост НИЖЕ последней цифры quot

// ОДНА десятичная доокруглка до p значащих цифр (§2.3); sticky участвует в «ровно половина»:
(rounded, d) = round_to_precision(quot, p, mc.rm, sign, sticky)  // d = отброшено цифр (+1 при carry 99→100)

result_scale = a.scale - b.scale + k - d
→ BigDecimal { mant: apply_sign(rounded, sign), scale: result_scale }
```

### 2.3. Округление — `round_to_precision(mant_abs, target_precision, rm, sign, sticky)`

Отбрасывание младших цифр НЕОТРИЦАТЕЛЬНОЙ мантиссы до `target_precision` значащих цифр.
Знак — ПАРАМЕТРОМ: (а) div_rem на отрицательных ломает сравнения с `half` (rem знака делимого);
(б) `Ceiling`/`Floor` обязаны смотреть на знак ИСХОДНОГО значения — знак усечённого quot
теряется на «-0.4 → quot 0» (Sign.Zero), и Floor не дал бы -1. `sticky` — есть ли ненулевой
хвост НИЖЕ отбрасываемых цифр (от деления §2.2; при прямом `@round` числа — false).
Возвращает `(rounded, d)` — d нужен вызывающему для коррекции scale.

```
d = mant_abs.digits() - target_precision
if d <= 0 → return (mant_abs, 0)         // уже не длиннее цели — guard (иначе 10^отрицательное)
factor = 10^d
(quot, rem) = mant_abs.div_rem(factor)    // mant_abs >= 0 → rem >= 0
half = factor / 2                          // = 5×10^{d-1}, точно
gt_half = rem > half || (rem == half && sticky)   // sticky сдвигает «ровно половину» в «больше»
eq_half = rem == half && !sticky
tail    = rem != 0 || sticky
carry = match rm:
    HalfEven => gt_half || (eq_half && quot.is_odd())
    HalfUp   => gt_half || eq_half
    HalfDown => gt_half
    Down     => false                      // truncate
    Up       => tail
    Ceiling  => sign == Pos && tail
    Floor    => sign == Neg && tail
rounded = quot + (carry ? 1 : 0)
if rounded.digits() > target_precision:    // carry 99→100
    rounded = rounded / 10                 // хвост после carry нулевой по построению
    d += 1
→ (rounded, d)
```

### 2.4. Нормализация — `BigDecimal.normalize()`

Удаление конечных десятичных нулей мантиссы (уменьшение `scale` без изменения значения).
Работает одинаково при любом знаке scale:

```
loop:
    if mant.is_zero() → return BigDecimal(mant=Zero, scale=0)
    (q, rem) = mant.div_rem(10)   // ВО ВРЕМЕННЫЕ — фиксировать деление можно только при rem == 0
    if rem != 0 → break            // последняя цифра не ноль; mant НЕ тронут
    mant = q
    scale -= 1
// (фикс ревью 2026-07-30: прежняя редакция писала `(mant, rem) = mant.div_rem(10)` ДО
//  проверки rem — перезаписывала мантиссу и ТЕРЯЛА последнюю ненулевую цифру: 123 → 12)
```

**⚠ O(n²):** BigInt `div_rem(10)` — полный проход по лимбам; нормализация числа с 500 конечными нулями стоит 500 полных делений. V1: корректность > скорость; V2 требует trailing-zero-count через степени 10 (бинарный поиск).

**Lazy normalize:** Нормализация НЕ вызывается в конструкторах и операциях (+/-/×). Только в `@equal`, `@hash` и при явном вызове пользователем. Паритет с Rust `bigdecimal`: после цепочки `a + b + c` значение сохраняет trailing zeros; первый `@equal`/`@hash` нормализует.

### 2.5. Конверсии

- `fn[T Ints] T @to_bigdecimal() -> BigDecimal` — `BigInt::from(T)`, `scale = 0`.
- `i128 @to_bigdecimal() -> BigDecimal` — явная перегрузка (i128 не член Ints; Ints может отсутствовать к моменту V1 — тогда все перегрузки явные).
- `str @to_bigdecimal() -> Result[BigDecimal, ParseBigDecimalError]` — см. 2.9 «Формат строки». (Result, НЕ Option — D325 R1 «Result = любая падающая операция», Option только genuine absence R4; эталоны: `to_version`/`to_complex`/`to_int`. Один структурный error-тип на домен.)
- `BigDecimal @to_str(scale_pad int = 0) -> str` (0 = без дополнения; `int? = None` — не-Nova-синтаксис):
  ```
  // ЗНАК СНАЧАЛА: div_rem на отрицательной мантиссе теряет «-0.5»
  // (trunc-к-нулю: (-5).div_rem(10) = (0, -5) — int_part = 0, знак пропал → печаталось бы "0.5").
  sign = if mant.is_neg() { "-" } else { "" };  m = |mant|
  if scale > 0:
      (int_part, frac) = m.div_rem(10^{scale})
      sign + int_part @to_str + '.' + pad_left(frac @to_str, scale, '0')
  if scale <= 0:
      // целое с неявными нулями: m × 10^{-scale}; при большом |scale| материализует
      // гигантский BigInt — документировано, V1 приемлемо
      sign + (m × 10^{-scale}) @to_str  // без точки
  ```
  Параметр `scale_pad` — минимальное число цифр после запятой (дополнение справа нулями).
- `BigDecimal @to_int() -> Option[int]` — fits-проверка (BigInt @to_int).
- `BigDecimal @to_i128() -> Option[i128]` — явная перегрузка (i128 не Ints).
- `BigDecimal @round(ctx: MathContext) -> BigDecimal` — округление до `ctx.precision` значащих цифр (precision-based, см. 2.3).
- `BigDecimal @scale(target: int, rm: RoundingMode) -> BigDecimal` — округление до `target` десятичных знаков (scale-based, установка свойства scale). Аналог Java `setScale(int, RoundingMode)`. Если `target > scale` — мантисса дополняется нулями (без округления); если `target < scale` — округление по `rm`. `target < 0` — округление до `10^{|target|}` (аналог Java `setScale(-2)` = to hundreds). **Имя пересмотреть на Ф.0:** поле `scale` уже даёт одноимённое свойство-читатель `@scale()` (D84); перегрузка того же имени операцией «установить с округлением» смешивает чтение свойства и вычисление — рекомендация **`@rescale(target, rm)`**.

### 2.6. Операторный desugar и равенство

**Desugar:** `+`/`-`/`*` → `@plus/@minus/@times`. `/` **НЕ десугарится** — `@div(b, mc)` требует явного `MathContext` (деление BigDecimal неоднозначно без точности). Оператор `a / b` в V1 не поддерживается; только `a.@div(b, mc)`.

**Равенство:** BigDecimal ОБЯЗАН объявить собственные `@equal`/`@hash`/`@compare` — структурное `==` value-record (D328) здесь НЕВЕРНО: lazy normalize допускает разные `(mant, scale)` у равных значений (`1.0 = {10,1}` vs `1 = {1,0}`), field-by-field дал бы `1.0 != 1`. **Ф.0-пин (блокер):** проверить пробой, что `==` при ОБЪЯВЛЕННОМ у типа `@equal` диспетчеризуется в него, а не в структурное; если структурное побеждает — вопрос владельцу ДО Ф.1. (Прежняя редакция утверждала одновременно «== структурное, #impl(Equal) не требуется» и «@equal нормализует оба операнда» — эти утверждения несовместимы.)

**Lazy normalize + @equal:** Поскольку normalize не вызывается в операциях, два значения с одинаковым числовым значением могут иметь разное mant+scale (`1.0 = {10, 1}`, `1 = {1, 0}`). Структурное `==` на value-record дало бы Java-gotcha. Решение:

- `@equal` нормализует ОБА операнда перед memberwise-сравнением (паритет Rust `bigdecimal`).
- `@hash` нормализует перед хэшированием.
- `@compare` — НЕ через нормализацию и НЕ лексикографически (фикс ревью 2026-07-30: normalize не уравнивает scale — `1.5={15,1}` vs `2={2,0}`, лексикографика по (scale, mant) сравнила бы 1.5 > 2). Правильно: сравнить знаки; при равных — выровнять scale (домножить мантиссу с МЕНЬШИМ scale на `10^diff`, как в сложении) и сравнить BigInt-мантиссы с учётом знака.

**⚠** Первый `@equal`/`@hash` на ненормализованном значении платит O(n²). Последующие вызовы — O(1), если значение уже нормализовано. Для V1 этого достаточно; V2 — кэш нормализованной формы в value-record или lazy-normalize-on-construction через trailing-zero-count.

**Прецедент:** `str @plus` (02-types.md ~9075) — value-record с heap-данными, `+` работает. Ф.0 подтверждает desugar для BigDecimal.

### 2.7. Литералов и коэрций НЕТ

Никакого неявного `int → BigDecimal` (аллокация ≠ zero-cost — вне полос D429 #coerce). Только явные `to_bigdecimal`-вызовы. Литеральная форма `12.345bd`-стиля — не в V1.

### 2.8. Дом — в репе `nova-bignum`

BigDecimal НЕ отдельная репа — живёт в той же репе `nova-bignum` (решение владельца: BigInt, BigDecimal, BigRat, BigFloat — все в одном внешнем пакете, как Go `math/big`). Подпакет `bigdecimal` рядом с `bigint`/`bigrat`/`bigfloat`. Тесты — `bigdecimal_test.nv` рядом.

### 2.9. Формат строки для `str @to_bigdecimal`

Возврат — `Result[BigDecimal, ParseBigDecimalError]` (см. 2.5); в таблице ниже `Err` = `Err(ParseBigDecimalError)`. Спецификация входного формата (не хуже Rust `bigdecimal`, но без Unicode-digit-сюрпризов Java):

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
- Ровно одна точка; второе вхождение `'.'` → `Err`. Ровно одна экспонента; второе `'e'`/`'E'` → `Err`.
- Пустой int-part или frac-part после удаления `_` → `Err` (кроме случая `'.'` с обеих сторон пусто → тоже `None`).
- Экспонента без digits после sign (или без sign и без digits) → `Err`.
- Scale = `len(frac_part_cleaned) - exp_value`. Scale может быть отрицательным (экспонента больше числа дробных цифр).
- Обработка только ASCII `'0'..'9'`. Не-ASCII цифры (арабские, деванагари) → `Err`.

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
| `"123.45.6"` | `Err` | две точки |
| `"1e"` | `Err` | пустая экспонента |
| `""` | `Err` | пустая строка |
| `"hello"` | `Err` | не-цифры |

## 3. Фазы

- **Ф.0 Разведка/дизайн-фиксация (короткая, после закрытия Plan 235):** (1) фикстура операторного desugar на value-record с `@plus/@compare` для BigDecimal — подтвердить, что `+`/`<` работают; (2) выбор `MathContext`-формата; (3) API-ревью владельцем; (4) ПИН `==`→`@equal`-диспетча на value-record с объявленным `@equal` (§2.6 — блокер: если структурное D328 побеждает, `1.0 != 1`); (5) имя `@rescale` vs `@scale` (§2.5); (6) пин-тест знаковой конвенции BigInt `@div_rem` (235: trunc-к-нулю, rem знака делимого) — на ней держатся §2.2-2.5 (все сравнения с half ведутся на |mant|).

- **Ф.1 Представление + нормализация + конверсии (sonnet):** `type BigDecimal value {…}`, `type MathContext`, `type RoundingMode`; `T @to_bigdecimal`/`i128 @to_bigdecimal`/`str @to_bigdecimal`; `to_str`/`normalize`.

- **Ф.2 Сложение, вычитание, умножение (sonnet):** выравнивание scale (BigInt `×10^k`), BigInt `@plus/@minus/@times`; все комбинации знаков (Sign-таблица); `@neg`, `@abs`, `@compare`, `@equal`. Тесты: PRNG-identity против `f64` на представимых значениях (без переполнения double) + канонические вектора (сумма/разность с разными scale, в т.ч. отрицательными).

- **Ф.3 Деление (sonnet, самый рискованный кусок):** расширение делимого на `p+2` цифры (p результат + 1 округление + 1 запас на carry), BigInt `@div_rem`, отбрасывание младшей цифры (`chop_last_digit`), округление по RoundingMode, коррекция scale при carry. Матрица 7 режимов × typical edge (0.5, 1.5, 2.5, -0.5, →∞/→0/→-∞). Проверка: `a / b * b` ≈ `a` в пределах точности.

- **Ф.4 MathContext + явное `@round` / `@scale` (sonnet):** precision-based `@round(ctx)`, scale-based `@scale(target, rm)`, цепные операции с контекстом.

- **Ф.5 Тесты (sonnet пишет карту+тонкие, haiku — вектора по литеральному образцу):** PRNG-identity против Java `BigDecimal`-эталона (по значениям, не по коду); edge-вектора: деление на ноль → panic (паритет int), `HALF_EVEN` banker's rounding (`2.5 → 2`, `3.5 → 4`), крайние scale (±10^5, 0), 0 в разных scale; отрицательный scale после `1.5E+2`; `to_str` round-trip (`x.to_str().to_bigdecimal() == x`); `@equal` compareTo-семантика (`1.0 == 1`); `@scale` с отрицательным target.

- **Ф.6 Закрытие:** doc-комменты, STATUS-строка, запись в simplifications при упрощениях.

## 4. Гейты

Таргетно: `nova test` репы `nova-bignum` зелёный; `--strict-effects`; линт чистый. Авторитетный (интегратор): conformance-CU не задет (внешняя репа).

## 5. Риски

| Риск | Митигация |
|---|---|
| Нормализация: `div_rem(10)` в цикле — O(n²) при большом числе конечных нулей | Вызывается только в `@equal`, `@hash` и явном `normalize()`. V1: корректность > скорость. Документировать: normalize дорог для чисел с >10⁴ trailing zeros |
| Деление — самая рискованная арифметика: k-расширение (учёт digits), sticky, carry 99→100 | Ф.3: worked-примеры (1/8, 100/8, 2/3, 1/3 при p=1..4) + sticky-вектора (2.5000001 при HalfEven → 3, не 2) + инвариант a.div(b,mc).times(b) ≈ a |
| Округление: `HALF_EVEN` требует проверки чётности — BigInt `@is_even()` за O(1) | `is_even` на BigInt тривиален |
| scale overflow: `add(a,b)` → `|a.scale ± b.scale|` может превысить `int::MAX` | Паритет Rust: scale = int (64-bit), overflow физически невозможен для представимых чисел (2⁶³ знака > атомов во вселенной). Документировать: не паниковать, обрезать до `int::MAX`? или panic? Рекомендация: panic по паритету `int` overflow в Nova |
| Операторы на value-record не десугарятся (`@plus`/`@times`) | Ф.0-пин; fallback = методы без операторов (`a.plus(b)` вместо `a + b`) |
| `Ints` type-set может быть не готов к V1 Nova → `fn[T Ints] T @to_bigdecimal` не скомпилируется | Явные перегрузки `i8 @to_bigdecimal`, … `u64 @to_bigdecimal` как fallback; generic — если доступен |
| `@scale(target, rm)` с отрицательным target: округление до `10^{|target|}` | Документировано в API. `BigDecimal(1234, 0).@scale(-2, DOWN)` = `BigDecimal(12, -2)` (= 1200). Тесты: Ф.5 |

## 6. Вне объёма (V2+ по отдельным решениям)

Оператор `/` (с thread-local или default `MathContext`); `pow`/`sqrt`; мутабельные `@add_assign`/`@sub_assign`/`@mul_assign` для GC-шума в циклах; цепные операции с автоматическим переносом точности (`a.plus(b, mc)` — однооконная форма, не `mc` как receiver); интеграция с generic-числовыми type-sets (D310); неявные коэрсии и литералы; кэш малых степеней 10; Toom-Cook для умножения; `remainder`/`divideToIntegralValue` (Java-style).

## Связи

[Plan 235](235-bigint.md) (зависимость — BigInt фундамент для мантиссы) · D423 (trap-политика ядра — BigDecimal наследует panic на div-by-zero) · D429 (почему коэрций нет: аллокация вне zero-cost-полосы) · Java `java.math.BigDecimal` / Python `decimal.Decimal` / Go `math/big` (эталоны API) · `std/src/math/int128.nv` (фиксированная ширина) · D310 (type-set bounds — интеграция в generic-арифметику V2).
