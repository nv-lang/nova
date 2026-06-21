<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 179 — Переработка системы времени: типизированный `Time`-эффект (retire int-wire) + Monotonic из builtin в `.nv` + единый источник схемы

> **Top-level план.** Создан 2026-06-22 (по аудиту time-подсистемы). **Статус:** 📋 PROPOSED.
> **Маркер:** `[M-179-time-system-rework]`. **Запуск:** «**выполни план 179**».
> **Координация:** record-через-границу-эффекта = Plan 172.4 (value-ABI auto-placement); схема-эффекта-из-`.nv`
> = Plan 172.1 (U.1/U.2 единое знание stdlib); effect-vtable storage = Plan 176. **Разблокирует** Plan 66
> (`tick_every`, см. [66 §3](66-timer-wheel-and-tick-every.md)). **Поглощает** `[M-time-now-schema-mismatch]`,
> `[M-monotonic-mock-support]`, `[M-monotonic-migration-deferred]`; финализирует `[M-handler-duration-schema-mismatch]`
> (partial) и `[M-monotonic-per-os-isolated-tests]`.
> **Сквозной критерий:** «без упрощений, как для прода».

## 1. Зачем (вердикт аудита 2026-06-22)

Time-типы в Nova концептуально правильные (`Duration`/`Timestamp`/`Monotonic` — знаковые i64-ns value-записи; D124
разделяет wall-clock и monotonic на уровне типов; `Time` — ambient suspend-эффект D64 → детерминизм в тестах через
handler-подмену). Но **поверхность эффекта рассинхронизирована и протекает сквозь нетипизированный int-провод**:

1. **`Time.now() -> int` — нетипизированный провод (`[M-time-now-schema-mismatch]`).** Эффект-схема возвращает
   `nova_int` (raw ns), а stdlib (`std/time/duration.nv`) и test-handler'ы объявляют `now() -> Timestamp`. Дрейф:
   `Time.now().minus(other)` роутится по **int-receiver path**, а не `Timestamp.@minus` → ломается method-dispatch.
   Workaround сейчас — руками оборачивать: `Timestamp.from_unix_millis(Time.now())`.
2. **`Monotonic.now()` — compiler-builtin, а не `.nv`.** Чтобы обойти тот же mismatch, монотонные часы захардкожены
   ([emit_c.rs:2312](../../compiler-codegen/src/codegen/emit_c.rs#L2312) + `Nova_Time_now_monotonic` в
   [fibers.h:2951](../../compiler-codegen/nova_rt/fibers.h#L2951)) вместо `=> Time.now_monotonic()`. Нарушает
   §3-правило «брать из `.nv` по максимуму» (см. [[feedback-maximize-nv-sourcing]]) и делает их **немокабельными**
   (`[M-monotonic-mock-support]`: `fixed`/`mut_clock` не перехватывают `Monotonic.now()` — всегда real `uv_hrtime`).
3. **Четыре расходящихся источника одной схемы.** prelude-декларация ([effects.nv:137](../../std/prelude/effects.nv#L137)
   `now()->int`, `sleep(ms int)`) ↔ codegen hardcode ([emit_c.rs:2297](../../compiler-codegen/src/codegen/emit_c.rs#L2297))
   ↔ handler-литералы ([handlers.nv:180](../../std/testing/handlers.nv#L180) `now()->Timestamp`, `sleep(d Duration)`)
   ↔ fibers.h builtin. Менять схему = править 4 места согласованно.
4. **`sleep(ms int)` vs `sleep(d Duration)`.** prelude-декларация берёт сырой `int` ms; handler'ы и реальный usage —
   `Duration`. Bridge `[M-handler-duration-schema-mismatch]` уже частично закрыт (annotation-мост в handler-body),
   но канон в декларации всё ещё `int`.
5. **Единица неоднозначна.** `now()->int` — это ms или ns? prelude-doc говорит «monotonic ms», `Timestamp`/`Monotonic`
   хранят ns. Источник unit-confusion.
6. **`Time.now()→Monotonic` миграция заморожена** (`[M-monotonic-migration-deferred]`, ≈9 timing-сайтов: rate_limiter,
   cancel_latency_bench, sleep_real_clock, …) — должны быть NTP/DST-immune, но блокированы mismatch'ем.
7. **Нет гражданского времени.** `Timestamp` — голые Unix-ns; нет Date/DateTime/TimeZone, format/parse (ISO-8601/
   RFC-3339), компонент (год/месяц/день…), нет BigDate за ±292 года. *(Большая аддитивная поверхность → под-план 179.1.)*

## 2. Текущая схема (как есть)

| Поверхность | `now` | `sleep` | monotonic | Источник |
|---|---|---|---|---|
| prelude decl | `now() -> int` | `sleep(ms int)` | — | [effects.nv:137](../../std/prelude/effects.nv#L137) |
| codegen wire | `now -> nova_int` | `sleep(nova_int) -> nova_unit` | `now_monotonic -> nova_int` | [emit_c.rs:2297](../../compiler-codegen/src/codegen/emit_c.rs#L2297) |
| test handlers | `now() => Timestamp` (+`now_ms`/`now_ns`) | `sleep(d Duration)` | — | [handlers.nv:180](../../std/testing/handlers.nv#L180) |
| runtime builtin | — | `nova_fiber_sleep_ms` | `Nova_Time_now_monotonic` (i64) | [fibers.h:2951](../../compiler-codegen/nova_rt/fibers.h#L2951) |
| stdlib типы | `Timestamp{nanos i64}` | — | `Monotonic{nanos i64}` (builtin `now()`) | [duration.nv](../../std/time/duration.nv) |

Плюс 5 observability-счётчиков в `effect_schemas["Time"]` (`timer_alloc_total/active/fired/cancelled/longest_pending_ms`,
все `nova_int`) — **не «время», а timer-runtime-интроспекция** (Plan 65 Ф.11). Семантически чужие `Time`.

## 3. Новая схема (типизированный эффект; один источник)

**Принцип:** эффект отдаёт **типизированные записи**, а не сырой int; единица — **наносекунды** внутри всех трёх
типов; схема живёт в **одном** месте (`.nv`-декларация), codegen её **читает** (не хардкодит); `Monotonic.now()` —
обычная `.nv`-функция; default-handler = тонкие non-portable extern-примитивы (C) + typed-обёртка в `.nv`.

```nova
export type Time effect {
    now()           -> Timestamp     // wall-clock (Unix epoch ns); может прыгать (NTP/DST)
    now_monotonic() -> Monotonic     // монотонные часы (ns); never backwards
    sleep(d Duration) -> ()          // suspend текущего fiber на d (D64)
}
```

| Операция | Было | Стало | Заметка |
|---|---|---|---|
| wall-clock | `now() -> int` (ms/ns?) | `now() -> Timestamp` | закрывает mismatch; `.minus()` → `Timestamp.@minus` |
| monotonic | builtin `Monotonic.now()` (i64) | effect-op `now_monotonic() -> Monotonic` + `.nv`-обёртка `Monotonic.now() => Time.now_monotonic()` | мокабельно; убирает hardcode |
| sleep | `sleep(ms int)` | `sleep(d Duration)` | финализирует `[M-handler-duration-schema-mismatch]` |
| observability | 5×int в `Time` | вынести в отдельный surface (как `Mem`) **или** пометить orthogonal | решение в Ф.0 |
| единица | ms vs ns дрейф | **ns** канон внутри всех типов | задокументировать |

**ABI-ключ (исправлено 2026-06-22 после проверки C).** `Timestamp`/`Monotonic`/`Duration` — все `{ ro nanos i64 }`,
но **сейчас это heap reference-records, не стек** (D215: `type X { … }` фигурными = heap, GC-managed; в сгенерённом
C — `Nova_Duration*`, см. [duration.c:1965](../../std/time/duration.c#L1965) `Nova_Duration* …from_nanos(...)`).
Единственный i64 завёрнут в кучу → расход на hot-path Duration-арифметике и **узкого scalar-bridge нет** (через границу
идёт указатель, а не i64). Поэтому Ф.2 предваряется **Ф.1b — миграцией этих трёх типов в `value`-records** (прецедент
Plan 165: Range/VecIter `value`): (a) stack-аллокация, zero-GC на арифметике; (b) только тогда «один i64 через границу
эффекта» становится правдой → узкий scalar-bridge валиден без полного 172.4. Полный путь (record-через-границу для
произвольных, многополевых записей) — Plan 172.4. Ф.0 фиксирует, каким путём идём (узкий bridge поверх value-миграции
vs полный 172.4).

## 3a. Методы `Timestamp` / `Monotonic`: есть → после рерайта

**Инвариант:** метод-surface сохраняется **1:1** — рерайт его не сокращает, а *чинит* (через int-провод сейчас часть
ломается о method-dispatch). Меняется только представление (Ф.1b: heap→value) и провод эффекта (Ф.2: int→typed).

**`Timestamp`** (`#stable 0.1`):

| Метод | Сигнатура | Было | После |
|---|---|---|---|
| `EPOCH` / `from_unix_secs/millis/nanos` | `(i64) -> Self` | работает | без изменений |
| `as_unix_secs/millis/nanos` | `-> i64` | работает | без изменений |
| `@plus(d Duration)` | `-> Timestamp` | работает | без изменений |
| `@minus(d Duration)` | `-> Timestamp` | работает | без изменений |
| `@minus(other Timestamp)` | `-> Duration` | работает (overload) | без изменений |
| `@compare` | `-> int` (синтез `==…>=`) | работает | без изменений |
| `@is_past` / `@time_until` / `@elapsed` | `() Time -> bool/Duration` | **ломается** (int-провод: `Time.now()` не `Timestamp`) | **начинает работать** (Ф.2) |

**`Monotonic`** (`#stable 0.6`):

| Метод | Сигнатура | Было | После |
|---|---|---|---|
| `Monotonic.now()` | `-> Self` | **compiler-builtin**, немокабелен | `.nv`-обёртка `=> Time.now_monotonic()`, мокабелен (Ф.3) |
| `@as_nanos` | `-> i64` | работает | без изменений |
| `@plus(d Duration)` / `@minus(d Duration)` | `-> Monotonic` | работает | без изменений |
| `@elapsed_since(other Monotonic)` | `-> Duration` | работает (named, не оператор) | без изменений; **+ опц. добавить симметричный `@minus(other Monotonic) -> Duration`** (см. ниже) |
| `@compare` | `-> int` | работает | без изменений |
| ⛔ `Monotonic ± Timestamp`, `as_unix_*`, `from_unix_*` | — | compile-error (D124) | **остаётся ошибкой** (намеренно) |

**Асимметрия `@elapsed_since` vs `@minus` (ответ на вопрос).** `Timestamp` имеет overload `@minus(Timestamp)->Duration`,
а `Monotonic` — нет (только named `@elapsed_since`). Причина в коде ([duration.nv:903](../../std/time/duration.nv#L903)):
*«Operator overload отсутствует, чтобы не конфликтовать с method-resolution на receiver type»*. Это **историческая
несимметрия** (Monotonic добавлен позже, Plan 65; автор обошёл второй `@minus`-overload + directional-имя читается
яснее: `t2.elapsed_since(t1)` однозначно `t2 − t1`, оператор легко перепутать). Но `Timestamp` тот же overload **несёт
и он работает** → запрет не фундаментальный. Ф.0 решает: дать `Monotonic` симметричный `@minus(Monotonic)->Duration`
(консистентность с Timestamp) **или** оставить `elapsed_since` каноном с явной фиксацией «directional by design». Под
unified-движком (172.1) overload-резолюция надёжна → технический блокер снимается.

## 4. Фазы

- **Ф.0 — Аудит + канон-решение (gate).** Зафиксировать: (a) канонический effect-surface (§3), (b) единица = ns,
  (c) куда деть 5 observability-счётчиков (вынести в `TimerMetrics`/`Mem`-style surface vs оставить orthogonal),
  (d) путь record-через-границу: полный 172.4 vs узкий single-i64-field bridge. Черновик D-block. **Без кода.**
- **Ф.1 — Единый источник схемы (убрать дрейф, без смены поведения).** Свести 4 поверхности к одной. Цель —
  codegen **читает** схему `Time` из `.nv`-декларации (коорд. 172.1 U.1/U.2), а не хардкодит. Минимум-инвариант
  (land сейчас): выровнять [emit_c.rs:2297](../../compiler-codegen/src/codegen/emit_c.rs#L2297) с prelude-decl и
  handler'ами так, чтобы они не расходились. Поведение не меняется.
- **Ф.1b — `Duration`/`Timestamp`/`Monotonic` → `value`-records.** Сейчас они heap reference-records (`{}`, D215 →
  `Nova_*`-указатель в C). Мигрировать на `value` (прецедент Plan 165). Выигрыш: (a) stack/zero-GC на hot-path
  Duration-арифметике; (b) разблокирует узкий scalar-bridge для Ф.2 (один i64 через границу эффекта). Watch: value-const
  (`const ZERO Duration = {…}`), `@into()`/`@into_human()` (возвращают `str`), `DurationParts` (7 полей — остаётся heap,
  это display-helper, не hot-path). Возможен codegen generic-forward-decl нюанс (D290, Plan 165) — здесь типы
  не-generic, проще. Координация 172.4 (единый value-ABI).
- **Ф.2 — Типизированная поверхность (retire int-wire).** `now() -> Timestamp`, `now_monotonic() -> Monotonic`,
  `sleep(d Duration)` через границу эффекта. `Time.now().minus(other)` роутится на `Timestamp.@minus`. **Закрывает
  `[M-time-now-schema-mismatch]`.** Gate: 172.4 (record-across-boundary) **или** узкий bridge из Ф.0.
- **Ф.3 — Monotonic из builtin → `.nv`.** Убрать compiler-builtin dispatch ([emit_c.rs:2312](../../compiler-codegen/src/codegen/emit_c.rs#L2312)
  + [fibers.h:2951](../../compiler-codegen/nova_rt/fibers.h#L2951)); `Monotonic.now() => Time.now_monotonic()` как
  Nova-fn в [duration.nv](../../std/time/duration.nv). **Закрывает `[M-monotonic-mock-support]`** (handler перехватывает).
- **Ф.4 — `sleep(Duration)` канон + unit-гигиена.** `sleep(ms int)` → `sleep(d Duration)` в prelude-decl;
  финализировать `[M-handler-duration-schema-mismatch]`; задокументировать ns-канон везде.
- **Ф.5 — Default + test handlers + миграция Time.now()→Monotonic.** Default-handler: тонкие extern-примитивы
  (`__nova_wall_now_ns`/`__nova_monotonic_now_ns`/`nova_fiber_sleep`, non-portable → C по [[feedback-maximize-nv-sourcing]]
  §3) + typed-обёртка в `.nv`; `fixed`/`mut_clock` под новую схему. Мигрировать ≈9 timing-сайтов на `Monotonic.now()`
  (**закрывает `[M-monotonic-migration-deferred]`**).
- **Ф.6 — Тесты + per-OS + spec/docs.** pos+neg (§7); per-OS monotonic-тест (`[M-monotonic-per-os-isolated-tests]`,
  опц. dedicated `nova_rt/time.c`); D-block amend; переписать `docs/` (strings-internals-аналог для time).

**Вне scope этого плана (отдельные под-планы):**
- **179.1 — Гражданское время** (эскиз → **§11**)**:** `Date`/`TimeOfDay`/`DateTime`/`ZonedDateTime`/`TimeZone`/`Period`,
  format+parse (ISO-8601/RFC-3339), компоненты (год/месяц/день/час…), BigDate (±292y overflow). Большая аддитивная
  поверхность поверх типизированного фундамента; эталон — java.time/Temporal.
- **Plan 66 — `tick_every` + timer-wheel** (этот план его разблокирует; не поглощаем).

## 5. Spec / D / Q / docs

- amend **D124** (Monotonic vs Timestamp): теперь оба возвращаются типизированно из `Time`-эффекта; `Monotonic.now()` —
  `.nv`-обёртка над `Time.now_monotonic()`, не builtin.
- amend **prelude `Time`-декларация** (D11/D14/D62, [04-effects.md]): typed-surface `now()->Timestamp` /
  `now_monotonic()->Monotonic` / `sleep(d Duration)`; единица — ns.
- **NEW D-block** — «`Time`-эффект: типизированный surface + единый источник схемы (codegen читает из `.nv`)»;
  фиксация ns-канона; политика observability-счётчиков. error-index: code на нарушение (если потребуется).
- сверить с **D64** (Time = suspend-effect, запрещён в `realtime {}`) — surface-смена не должна снять запрет.
- `docs/` — обновить раздел про время (по образцу [docs/strings-internals.md](../strings-internals.md)); таблица
  «было→стало»; убрать упоминания int-провода как «текущего».

## 6. Миграция (§7 compiler-conventions)

nv не в релизе → меняем напрямую, но **измерить blast-radius** перед сменой surface: сколько сайтов зовут `Time.now()`
(ожидают int vs Timestamp), `Time.sleep(...)`, `Monotonic.now()`, и handler-литералов с `Time` (std + nova_tests).
Переписать в том же изменении. Верификация — против чистого бинаря (kill-switch на том же билде, см.
[[feedback-codegen-dce-verification]]).

## 7. Тесты (pos + neg)

- **pos** `nova_tests/time179/`: `Time.now().elapsed()/.minus()/.time_until()` без ручной обёртки (роутинг на
  `Timestamp`-методы); `Monotonic.now().elapsed_since()`; `sleep(Duration)`; mock через `fixed`/`mut_clock` —
  **в т.ч. перехват `Monotonic.now()`** (раньше невозможно); `with Time = ...` детерминизм; D124 cross-type
  по-прежнему compile-error.
- **neg:** смешивание `Monotonic`↔`Timestamp` арифметикой → compile-error (absent overload, D124); `Time` внутри
  `realtime {}` → `E_EFFECT_REALTIME_VIOLATION` (D64); (если вводим) reinterpret raw-ns без явного метода → ошибка.
- per-OS: монотонность (`now_monotonic` не убывает) на Win/Linux; wall vs monotonic не путаются.

## 8. Критерии приёмки

1. `Time`-эффект типизирован: `now()->Timestamp`, `now_monotonic()->Monotonic`, `sleep(Duration)`; int-провод ретайрнут.
2. Схема `Time` — **один** источник (codegen читает из `.nv`, не хардкодит 4-ю копию).
3. `Monotonic.now()` — `.nv`-функция (builtin-dispatch удалён); мокабелен через handler.
4. `Time.now().minus(...)`/`.elapsed()` работают без ручной обёртки; ≈9 timing-сайтов мигрированы на `Monotonic`.
5. Закрыты `[M-time-now-schema-mismatch]`, `[M-monotonic-mock-support]`, `[M-monotonic-migration-deferred]`;
   финализирован `[M-handler-duration-schema-mismatch]`.
6. Полный регресс зелёный; **без упрощений**.

## 9. Конвенции + координация

§1 (проверки в чекере), §3 (схема/типы из `.nv`/реестра, не хардкод — прямо мотив Ф.1/Ф.3), §5 spec-first
(D-block до кода), §6 (коды ошибок + error-index), §7 (blast-radius + чистый бинарь), §8 (pos+neg, C-codegen).
**Координировать с 172.4** (record-через-границу-эффекта — ключевой gate Ф.2) и **172.1** (схема эффекта как часть
единого знания stdlib, U.1/U.2). `02-types.md`/effect-схему в codegen — не править в одиночку (зона 172-переработки).
**Разблокирует Plan 66** (`tick_every` ждёт этот mismatch).

## 10. Followup

`[M-179-time-system-rework]`. Поглощает `[M-time-now-schema-mismatch]` (Ф.2), `[M-monotonic-mock-support]` (Ф.3),
`[M-monotonic-migration-deferred]` (Ф.5). Гражданское время → под-план **179.1** (§11). `tick_every` → **Plan 66**.
Имена/детали финал — при реализации (после Ф.0 канон-решения).

## 11. Под-план 179.1 — гражданское время (эскиз, не в scope 179)

> **Статус:** эскиз/proposal. Самостоятельный под-план поверх типизированного фундамента 179. **Не специфицирован** —
> ниже только направление + откуда берём эталон.

**Откуда «идеал».** Беру за референс самые выверенные civil-time дизайны (а не из головы):
- **java.time (JSR-310)**, Stephen Colebourne — де-факто золотой стандарт: `Instant`/`LocalDate`/`LocalTime`/
  `LocalDateTime`/`ZonedDateTime`/`OffsetDateTime`/`ZoneId`/`Duration`/`Period`.
- **TC39 Temporal** (JS, современный редизайн 2020-х) — `Instant`/`PlainDate`/`PlainTime`/`PlainDateTime`/
  `ZonedDateTime`/`Duration`; чистое именование «Plain» = «без зоны».
- **Noda Time** (.NET, тот же Colebourne-подход), **Rust `time`/`chrono`**, **Go `time`**.
- Стандарты: **ISO 8601** (формат), **RFC 3339** (internet-timestamps), **IANA tz database** (зоны).

**Три урока, которые беру оттуда:**
1. **Instant ≠ civil.** Точка на оси времени (`Timestamp` — у нас уже есть, = Instant) отделена от «человеческих» полей
   (год/месяц/день). Не смешивать Unix-наносекунды с календарём.
2. **Plain vs Zoned.** `PlainDateTime` (без зоны, «настенный календарь») отдельно от `ZonedDateTime` (= `PlainDateTime` +
   `TimeZone`, резолвится в `Timestamp`). Никаких неявных дефолтных зон.
3. **`Duration` ≠ `Period`.** `Duration` = точный elapsed (наносекунды, **у нас уже есть**); `Period` = календарная
   величина (мес./годы — переменной длины). Арифметика «+1 месяц» требует `Period`. **У Nova `Period` нет — это пробел.**

**Предполагаемый набор (proposal, имена под bikeshed; `Time` занят эффектом → civil-«время суток» = `TimeOfDay`):**

| Тип | Аналог java.time / Temporal | Поля / суть | Ключевые методы (эскиз) |
|---|---|---|---|
| `Timestamp` *(есть)* | `Instant` | Unix ns | + `to_zoned(tz) -> ZonedDateTime`, `to_rfc3339() -> str`, `Timestamp.parse_rfc3339(s)` |
| `Date` | `LocalDate`/`PlainDate` | year/month/day | `.year/.month/.day`, `.weekday()`, `.plus(Period)`, `.iso()`, `Date.parse_iso(s)` |
| `TimeOfDay` | `LocalTime`/`PlainTime` | hh/mm/ss/ns | `.hour/.minute/.second/.nano`, `.plus(Duration)` |
| `DateTime` | `LocalDateTime`/`PlainDateTime` | `Date` + `TimeOfDay`, без зоны | `.date()/.time()`, `.plus(Period\|Duration)`, format/parse ISO-8601 |
| `ZonedDateTime` | `ZonedDateTime` | `DateTime` + `TimeZone` | `.to_instant() -> Timestamp`, `.offset()`, arithmetic с DST-resolution |
| `Offset` | `ZoneOffset` | фикс. UTC-сдвиг (+03:00) | `.total_seconds()` |
| `TimeZone` | `ZoneId` | IANA-имя + правила | `TimeZone.of("Europe/Moscow")`, `.offset_at(Timestamp)` |
| `Period` ⚠ **новый** | `Period` | years/months/days (variable) | `.plus`, отличён от `Duration` на уровне типа |

**Открытые Q для 179.1:** именование (`TimeOfDay` vs `Clock` vs `WallTime`); нужен ли IANA tz-db в рантайме (вес!) vs
только фикс-`Offset` в MVP; `Period`-арифметика и разрешение неоднозначностей DST (gap/overlap); BigDate за ±292 года
(i128 или раздельный epoch-day + ns-of-day). Scope режется отдельно при заведении под-плана.
