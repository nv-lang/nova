<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Гражданское (календарное) время в Nova — `std/time/civil`

**English** | [Русский](datetime.ru.md)

> Plan [175.1](../plans/175.1-civil-time.md); нормативные решения — D319/D320/D321
> ([spec/decisions/04-effects.md](../../spec/decisions/04-effects.md)).
> Instant/interval-слой (`Timestamp`/`Duration`/`Monotonic`) — Plan 175, D316-D318.

## Модель: type-ladder (D319)

```
Plain (не точка на оси)      Offset (точка, фикс. сдвиг)     Zoned (точка, rule-aware)
Date / TimeOfDay / DateTime → ZonedDateTime{zone: Fixed/Utc} → ZonedDateTime{zone: Iana}
```

- **Plain** (`Date`, `TimeOfDay`, `DateTime`) — календарные значения без зоны.
  «2026-07-10 14:30» — это НЕ момент: в Токио и Нью-Йорке он наступает в разное
  время. Поэтому Plain → `Timestamp` **только** через явную зону + политику
  разрешения DST, и это **fallible** (`Result`). Неявной «локальной зоны» нет.
- **`ZonedDateTime`** — момент: wall-время + **резолвнутый** `offset` + `zone`.
  Offset хранится в значении → round-trip безопасен даже при смене правил зоны.
- Отдельного `OffsetDateTime` нет: `ZonedDateTime{zone: Fixed(off)}` покрывает
  (jiff-подход, меньше поверхности).

Все типы — **value-records** (stack, zero-GC, структурное `==` через `@compare`).
Календарь — **proleptic Gregorian**, год 0 = 1 BCE (Q10); других календарей нет.
`Date` — `epoch_day i64` (дни от 1970-01-01), конверсия — алгоритмы Хиннанта
`days_from_civil`/`civil_from_days`. Leap-секунды игнорируются (день = 86400s);
`:60` при парсинге клампится к `:59` (Q5).

## Быстрый старт

```nova
import std.time.civil

ro d = Date.new(2026, Jul, 10)!!                 // Result: Feb-30 -> Err, не normalize
ro t = TimeOfDay.new(14, 30)!!                   // дефолты: s = 0, ns = 0
ro dt = d.at(t)                                  // DateTime (plain)

ro ny = "America/New_York".to_timezone()!!       // §1а: конверсия на источнике
ro z = dt.to_zoned(ny)!!                         // Compatible-политика DST
ro ts = z.to_timestamp()!!                       // Err вне окна ±292y

ro back = "2026-07-10T14:30:00-04:00[America/New_York]".to_zoned_datetime()!!
assert(back.compare(z) == 0)                     // RFC-9557 round-trip
```

## `Period` ≠ `Duration` (D320) — compile-time разделение

| Приёмник | `Period` (календарный y/m/d) | `Duration` (точные ns) |
|---|---|---|
| `Date` | ✅ `d + Period.of_months(1)` | ❌ compile-error |
| `TimeOfDay` | ❌ compile-error | ✅ `(t, carry) = t.plus(1.hour())` — day-carry ЯВНЫЙ |
| `DateTime` | ✅ `@plus(Period)` | ✅ `@plus(Duration)` |
| `ZonedDateTime` | ✅ `@plus(Period)` — wall-preserving | ✅ `@plus_duration` — elapsed |

Это чинит худший задокументированный изъян Temporal (единая Duration с
рантайм-`RangeError`) и отсутствие `Period` в Go.

### Wall vs elapsed (асимметрия Zoned-арифметики)

```nova
ro before = Date.new(2026, Mar, 7)!!.at(TimeOfDay.new(12, 0)!!).to_zoned(ny)!!
ro wall = before + Period.of_days(1)         // завтра 12:00 wall; elapsed = 23h (spring-forward)
ro exact = before.plus_duration(24.hours())  // ровно 24h; wall станет 13:00
```

### Clamp-арифметика (Q7) — non-invertible by design

`Jan31 + 1mo = Feb28/29` (clamp к последнему валидному дню), biggest-unit-first
(годы+месяцы одним сдвигом, затем дни точно). Следствие:
`(Jan31 + 1mo) - 1mo == Jan28` — календарная арифметика **необратима** на
clamp-границах (fixture в `civil_arith_test.nv`). 3-tier (D317): операторы
`+`/`-` **трапают** на выходе за civil-диапазон; `@try_plus` → `Result`;
`@saturating_plus` → clamp к `Date.min_value()`/`max_value()`.

Разности: `Period.between(start, end)` — календарная (инвариант
`start + between(start,end) == end`); `@days_until` — точные дни.

## DST: 4-way `Disambiguation` (D321)

При прикреплении plain-времени к зоне wall-time может не существовать
(spring-forward gap) или существовать дважды (fall-back overlap):

```nova
ro in_gap = Date.new(2026, Mar, 8)!!.at(TimeOfDay.new(2, 30)!!)  // NY: 02:00-03:00 съедены
ny.resolve_local(in_gap).is_gap()               // ambiguity как ЗНАЧЕНИЕ
in_gap.to_zoned(ny)!!                           // Compatible: push -> 03:30 EDT
in_gap.to_zoned(ny, Earlier)!!                  // 01:30 EST
in_gap.to_zoned(ny, Reject)                     // Err(Ambiguous)
```

`Compatible` (default, Temporal/RFC-5545-паритет): gap → сдвиг вперёд на длину
gap; overlap → ранний offset. `Utc`/`Fixed` неоднозначности не имеют.

Парсинг zoned-строк — отдельная политика `OffsetConflict`
(`RejectMismatch`(default)/`Use`/`Prefer`/`Ignore`) для рассогласования
сохранённого offset с текущими правилами зоны (tzdb-drift).

## Парсинг и формат (Ф.5; §1а: методы на источнике)

| Строка | Метод | Результат |
|---|---|---|
| `"2026-07-10"` | `.to_date()` | `Result[Date, ParseDateTimeError]` |
| `"14:30:05.250"` | `.to_time_of_day()` | `Result[TimeOfDay, _]` |
| `"2026-07-10T14:30:00"` | `.to_datetime()` | `Result[DateTime, _]` (plain, без зоны) |
| `"...T11:30:00Z"` | `.to_timestamp()` | `Result[Timestamp, _]` (RFC-3339) |
| `"...T14:30:00-04:00[America/New_York]"` | `.to_zoned_datetime()` | `Result[ZonedDateTime, _]` (RFC-9557) |
| `"P1Y2M3D"` / `"P2W"` | `.to_period()` | `Result[Period, _]` |
| `"Europe/Moscow"` / `"+05:30"` | `.to_timezone()` | `Result[TimeZone, DateError]` |

**Strict by default** (Q9): «parses» == «constructs» — `"2024-02-30".to_date()`
→ `Err(InvalidValue(_, Day, 30))`, обрезки/хвосты → `FormatMismatch(позиция)`;
zoned-строка обязана нести и offset, и `[zone]`. Единственное послабление —
leap-second `:60` → `:59`.

Обратно: `@to_iso()` на каждом типе (round-trip гарантирован), `Timestamp
@to_rfc3339()`, `ZonedDateTime @to_iso()` с RFC-9557 `[zone]`-суффиксом.

### Кастомный паттерн — type-safe builder (Q3, НЕ strftime / НЕ Go-layout)

```nova
ro f = DateTimeFormat.new().day2().lit(".").month2().lit(".").year4()
f.format(dt)                     // "10.07.2026"
"10.07.2026".to_date_with(f)!!   // обратно; опечатка в директиве = ошибка компиляции
```

Директивы: `year4/month2/day2/hour2/minute2/second2/frac3/month_name/
weekday_name/lit` + `opt_start()/opt_end()` (optional-секции, backtracking
parse). Собранные поля проходят ту же строгую валидацию, что конструкторы.

## Таймзоны: слои загрузки (D321 §tzdb)

1. `s.to_timezone()` — чистая embedded curated-таблица: `UTC`/алиасы,
   фикс-оффсеты, `America/New_York`, `Europe/London`, `Europe/Moscow`,
   `Australia/Sydney` (транзишены генерируются rule-based на 1996..2100).
2. `TimeZone.from_tzif(id, bytes)` — raw-TZif (RFC 8536 v1/v2/v3).
3. `load_timezone(name) Fs Os` — `$ZONEINFO` → `/usr/share/zoneinfo` (POSIX) →
   embedded. На Windows работает embedded-fallback.

`tzdb_version()` — версия curated-таблицы. Полный IANA-snapshot —
`[M-175.1-full-tzdb-embed]`.

## Часы и мокабельность

```nova
with Time = th.fixed_ms(1_700_000_000_000) {
    ro z = ZonedDateTime.now(Utc)          // детерминированно
    ro today = Date.today(ny)              // тоже
}
```

`Offset.local()` (D316 amend + D321, 2026-07-10 — closes
`[M-175.1-local-offset-effect-op]`) даёт системный UTC-сдвиг машины поверх
эффект-опа `Time.local_offset_sec()` (мокабелен, см. `docs/guide/time.md`). Это
ТОЛЬКО числовой сдвиг — зона в `ZonedDateTime` остаётся явной (D319 R1):
`dt.to_zoned(TimeZone.Fixed(Offset.local()))`, никакого implicit-fallback.

## Nova ↔ java.time (шпаргалка)

| java.time | Nova | Отличие |
|---|---|---|
| `LocalDate` | `Date` | value-record, `epoch_day i64` |
| `LocalTime` | `TimeOfDay` | day-carry в `@plus` ЯВНЫЙ (кортеж) |
| `LocalDateTime` | `DateTime` | — |
| `ZonedDateTime`/`OffsetDateTime` | `ZonedDateTime` (+`Fixed`) | один тип |
| `Period` | `Period` | == покомпонентное, normalize только m↔y |
| `Duration` | `Duration` (Plan 175) | — |
| `ZoneId`/`ZoneOffset` | `TimeZone`/`Offset` | sum-type |
| `DateTimeException` (throw) | `DateError` (Result) | значение, не исключение |
| earlier/later вручную | `Disambiguation` 4-way | Compatible default |
| `DateTimeFormatter` pattern-строка | `DateTimeFormat` builder | compile-checked |

## Footguns, задокументированные явно

- **ISO week-year ≠ calendar year** у границ года: `2027-01-01` принадлежит
  ISO-неделе 53 week-year'а **2026** (`@iso_week_year()` vs `@year()` — аналог
  java.time `Y` vs `y`). Fixture в `civil_test.nv`.
- Календарная арифметика non-invertible/non-associative на clamp-границах
  (см. выше).
- `Period` `==` — структурное: `1y != 12mo`; календарного порядка `<` без
  опорной даты не существует (структурный total-order для ключей — есть).
- Plain `DateTime @duration_until` осмыслен только без DST-переходов между
  значениями (plain не знает offset-скачков) — для моментов используйте
  `ZonedDateTime`/`Timestamp`.

## Дифференциаторы (Plan 175.1 §1a — что Nova делает лучше peers)

1. **Compile-time `Period`/`Duration` split** — «add hours to a Date» не
   компилируется (Temporal — рантайм-ошибка, Go — молчит).
2. **Type-ladder Plain/Zoned** — «хранить wall-clock как момент» = ошибка типов
   (корень Go-багов с единым `time.Time`).
3. **Value-records** — stack, zero-GC, дешевле heap-объектов Java.
4. **4-way DST как Result-значение** — строже java.time/Go/kotlinx/chrono.
5. **Effect-handler clock** — `now()` мокается детерминированно без DI-обвязки.
6. **Structured `DateError`** — ошибки значениями (D325), не `Invalid Date`/throw.
