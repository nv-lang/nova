<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Civil (calendar) time in Nova — `std/time/civil`

**English** | [Русский](datetime.ru.md)

> Plan [175.1](../plans/175.1-civil-time.md); the normative decisions —
> D319/D320/D321 ([spec/decisions/04-effects.md](../../spec/decisions/04-effects.md)).
> The instant/interval layer (`Timestamp`/`Duration`/`Monotonic`) — Plan 175, D316-D318.

## Model: the type ladder (D319)

```
Plain (not a point on the axis)      Offset (a point, fixed shift)     Zoned (a point, rule-aware)
Date / TimeOfDay / DateTime → ZonedDateTime{zone: Fixed/Utc} → ZonedDateTime{zone: Iana}
```

- **Plain** (`Date`, `TimeOfDay`, `DateTime`) — calendar values with no zone.
  "2026-07-10 14:30" is NOT an instant: in Tokyo and New York it arrives at a
  different time. So Plain → `Timestamp` **only** through an explicit zone +
  a DST resolution policy, and it's **fallible** (`Result`). There is no
  implicit "local zone".
- **`ZonedDateTime`** — an instant: wall time + a **resolved** `offset` +
  `zone`. The offset is stored in the value → round-trip is safe even if the
  zone's rules change.
- There is no separate `OffsetDateTime`: `ZonedDateTime{zone: Fixed(off)}`
  covers it (the jiff approach, less surface).

All types are **value records** (stack, zero-GC, structural `==` via
`@compare`). The calendar is **proleptic Gregorian**, year 0 = 1 BCE (Q10);
no other calendars exist. `Date` is `epoch_day i64` (days since 1970-01-01),
conversion via Howard Hinnant's `days_from_civil`/`civil_from_days`
algorithms. Leap seconds are ignored (a day = 86400s); `:60` when parsing is
clamped to `:59` (Q5).

## Quick start

```nova
import std.time.civil

ro d = Date.new(2026, Jul, 10)!!                 // Result: Feb-30 -> Err, not normalize
ro t = TimeOfDay.new(14, 30)!!                   // defaults: s = 0, ns = 0
ro dt = d.at(t)                                  // DateTime (plain)

ro ny = "America/New_York".to_timezone()!!       // §1a: conversion at the source
ro z = dt.to_zoned(ny)!!                         // the Compatible DST policy
ro ts = z.to_timestamp()!!                       // Err outside the ±292y window

ro back = "2026-07-10T14:30:00-04:00[America/New_York]".to_zoned_datetime()!!
assert(back.compare(z) == 0)                     // RFC-9557 round-trip
```

## `Period` ≠ `Duration` (D320) — a compile-time split

| Receiver | `Period` (calendar y/m/d) | `Duration` (exact ns) |
|---|---|---|
| `Date` | ✅ `d + Period.of_months(1)` | ❌ compile-error |
| `TimeOfDay` | ❌ compile-error | ✅ `(t, carry) = t.plus(1.hour())` — day-carry is EXPLICIT |
| `DateTime` | ✅ `@plus(Period)` | ✅ `@plus(Duration)` |
| `ZonedDateTime` | ✅ `@plus(Period)` — wall-preserving | ✅ `@plus_duration` — elapsed |

This fixes Temporal's worst documented flaw (a single Duration with a
runtime `RangeError`) and Go's lack of a `Period`.

### Wall vs. elapsed (the asymmetry of Zoned arithmetic)

```nova
ro before = Date.new(2026, Mar, 7)!!.at(TimeOfDay.new(12, 0)!!).to_zoned(ny)!!
ro wall = before + Period.of_days(1)         // tomorrow 12:00 wall; elapsed = 23h (spring-forward)
ro exact = before.plus_duration(24.hours())  // exactly 24h; wall becomes 13:00
```

### Clamp arithmetic (Q7) — non-invertible by design

`Jan31 + 1mo = Feb28/29` (clamps to the last valid day), biggest-unit-first
(years+months in one shift, then days exactly). Consequence:
`(Jan31 + 1mo) - 1mo == Jan28` — calendar arithmetic is **non-invertible** at
clamp boundaries (fixture in `civil_arith_test.nv`). 3-tier (D317): the
`+`/`-` operators **trap** on going outside the civil range; `@try_plus` →
`Result`; `@saturating_plus` → clamp to `Date.min_value()`/`max_value()`.

Differences: `Period.between(start, end)` — calendar-based (the invariant
`start + between(start,end) == end`); `@days_until` — exact days.

## DST: 4-way `Disambiguation` (D321)

When attaching a plain time to a zone, the wall time may not exist
(spring-forward gap) or may exist twice (fall-back overlap):

```nova
ro in_gap = Date.new(2026, Mar, 8)!!.at(TimeOfDay.new(2, 30)!!)  // NY: 02:00-03:00 is eaten
ny.resolve_local(in_gap).is_gap()               // ambiguity as a VALUE
in_gap.to_zoned(ny)!!                           // Compatible: push -> 03:30 EDT
in_gap.to_zoned(ny, Earlier)!!                  // 01:30 EST
in_gap.to_zoned(ny, Reject)                     // Err(Ambiguous)
```

`Compatible` (default, Temporal/RFC-5545 parity): gap → shift forward by the
gap's length; overlap → the earlier offset. `Utc`/`Fixed` have no ambiguities.

Parsing zoned strings has a separate `OffsetConflict` policy
(`RejectMismatch` (default)/`Use`/`Prefer`/`Ignore`) for a mismatch between
the stored offset and the zone's current rules (tzdb drift).

## Parsing and formatting (methods on the source)

| String | Method | Result |
|---|---|---|
| `"2026-07-10"` | `.to_date()` | `Result[Date, ParseDateTimeError]` |
| `"14:30:05.250"` | `.to_time_of_day()` | `Result[TimeOfDay, _]` |
| `"2026-07-10T14:30:00"` | `.to_datetime()` | `Result[DateTime, _]` (plain, no zone) |
| `"...T11:30:00Z"` | `.to_timestamp()` | `Result[Timestamp, _]` (RFC-3339) |
| `"...T14:30:00-04:00[America/New_York]"` | `.to_zoned_datetime()` | `Result[ZonedDateTime, _]` (RFC-9557) |
| `"P1Y2M3D"` / `"P2W"` | `.to_period()` | `Result[Period, _]` |
| `"Europe/Moscow"` / `"+05:30"` | `.to_timezone()` | `Result[TimeZone, DateError]` |

**Strict by default** (Q9): "parses" == "constructs" —
`"2024-02-30".to_date()` → `Err(InvalidValue(_, Day, 30))`, truncation/trailing
garbage → `FormatMismatch(position)`; a zoned string must carry both an
offset and a `[zone]`. The one concession is leap-second `:60` → `:59`.

Back the other way: `@to_iso()` on every type (round-trip guaranteed),
`Timestamp @to_rfc3339()`, `ZonedDateTime @to_iso()` with an RFC-9557
`[zone]` suffix.

### Custom pattern — a type-safe builder (Q3, NOT strftime / NOT a Go layout)

```nova
ro f = DateTimeFormat.new().day2().lit(".").month2().lit(".").year4()
f.format(dt)                     // "10.07.2026"
"10.07.2026".to_date_with(f)!!   // back; a typo in the directive = a compile error
```

Directives: `year4/month2/day2/hour2/minute2/second2/frac3/month_name/
weekday_name/lit` + `opt_start()/opt_end()` (optional sections, backtracking
parse). Assembled fields go through the same strict validation as the
constructors.

## Timezones: loading layers (D321 §tzdb)

1. `s.to_timezone()` — a pure embedded curated table: `UTC`/aliases, fixed
   offsets, `America/New_York`, `Europe/London`, `Europe/Moscow`,
   `Australia/Sydney` (transitions are generated rule-based over 1996..2100).
2. `TimeZone.from_tzif(id, bytes)` — raw TZif (RFC 8536 v1/v2/v3).
3. `load_timezone(name) Fs Os` — `$ZONEINFO` → `/usr/share/zoneinfo` (POSIX)
   → embedded. On Windows the embedded fallback is what runs.

`tzdb_version()` — the curated table's version. A full IANA snapshot is
`[M-175.1-full-tzdb-embed]`.

## Clocks and mockability

```nova
with Time = th.fixed_ms(1_700_000_000_000) {
    ro z = ZonedDateTime.now(Utc)          // deterministic
    ro today = Date.today(ny)              // also
}
```

`Offset.local()` (D316 amend + D321, 2026-07-10 — closes
`[M-175.1-local-offset-effect-op]`) gives the machine's system UTC offset on
top of the effect op `Time.local_offset_sec()` (mockable, see
`docs/guide/time.md`). This is ONLY a numeric offset — the zone in
`ZonedDateTime` stays explicit (D319 R1):
`dt.to_zoned(TimeZone.Fixed(Offset.local()))`, no implicit fallback.

## Nova ↔ java.time (cheat sheet)

| java.time | Nova | Difference |
|---|---|---|
| `LocalDate` | `Date` | value record, `epoch_day i64` |
| `LocalTime` | `TimeOfDay` | day-carry in `@plus` is EXPLICIT (a tuple) |
| `LocalDateTime` | `DateTime` | — |
| `ZonedDateTime`/`OffsetDateTime` | `ZonedDateTime` (+`Fixed`) | one type |
| `Period` | `Period` | == component-wise, normalize only m↔y |
| `Duration` | `Duration` (Plan 175) | — |
| `ZoneId`/`ZoneOffset` | `TimeZone`/`Offset` | sum-type |
| `DateTimeException` (throw) | `DateError` (Result) | a value, not an exception |
| earlier/later manually | `Disambiguation` 4-way | Compatible default |
| `DateTimeFormatter` pattern string | `DateTimeFormat` builder | compile-checked |

## Footguns, explicitly documented

- **ISO week-year ≠ calendar year** at year boundaries: `2027-01-01` belongs
  to ISO week 53 of week-year **2026** (`@iso_week_year()` vs `@year()` —
  the analog of java.time `Y` vs `y`). Fixture in `civil_test.nv`.
- Calendar arithmetic is non-invertible/non-associative at clamp boundaries
  (see above).
- `Period` `==` is structural: `1y != 12mo`; a calendar order `<` doesn't
  exist without an anchor date (a structural total order for keys does
  exist).
- Plain `DateTime @duration_until` is meaningful only with no DST
  transitions between the values (plain doesn't know about offset jumps) —
  for instants use `ZonedDateTime`/`Timestamp`.

## Differentiators (Plan 175.1 §1a — what Nova does better than its peers)

1. **Compile-time `Period`/`Duration` split** — "add hours to a Date" doesn't
   compile (Temporal — a runtime error, Go — stays silent).
2. **The Plain/Zoned type ladder** — "storing a wall clock as an instant" =
   a type error (the root of Go's bugs with a single `time.Time`).
3. **Value records** — stack, zero-GC, cheaper than Java's heap objects.
4. **4-way DST as a Result value** — stricter than java.time/Go/kotlinx/chrono.
5. **Effect-handler clock** — `now()` is mocked deterministically with no DI
   wiring.
6. **Structured `DateError`** — errors as values (D325), not `Invalid Date`/throw.
