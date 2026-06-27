<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 179 — Переработка системы времени: типизированный `Time`-эффект (retire int-wire) + overflow-safe Duration + Monotonic из builtin в `.nv` + единый источник схемы

> **Top-level план.** Создан 2026-06-22; production-hardened 2026-06-22 (cross-lang аудит Go/Rust/TS/Kotlin/Java +
> adversarial-критика, workflow `plan179-harden`). **Статус:** 📋 READY (все Q закрыты, см. §3.0).
> **Маркер:** `[M-179-time-system-rework]`. **Запуск:** «**выполни план 179**» (план самодостаточен — вся информация ниже).
> **D-блоки (NEW):** D316 (typed Time-surface + единый источник), D317 (Duration/instant overflow-policy), D318
> (Monotonic non-regression contract). Amend: D124, D237, prelude-`Time`-decl. (Свободные после D315=172.4; сверить при spec-шаге.)
> **Координация:** record-через-границу = решено **узкий single-i64 scalar-bridge поверх value-migration** (НЕ блокируемся на
> 172.4, см. §3.0-Q2); схема-из-`.nv` = коорд. Plan 172.1 (U.1/U.2); effect-vtable storage = Plan 174.4. **Разблокирует**
> Plan 66 (`tick_every`). **Поглощает** `[M-time-now-schema-mismatch]`, `[M-monotonic-mock-support]`,
> `[M-monotonic-migration-deferred]`; финализирует `[M-handler-duration-schema-mismatch]`, `[M-monotonic-per-os-isolated-tests]`.
> **Фоновые агенты:** см. §10 (НЕ `git stash`; temp-worktree/commit-reset; идемпотентность под rate-limit).
> **Сквозной критерий (обязательный):** «**без упрощений, как для прода**» — формальный критерий приёмки §8.0.

---

## 1. Зачем (вердикт аудита 2026-06-22)

Time-типы Nova концептуально правильные (`Duration`/`Timestamp`/`Monotonic` — знаковые i64-ns записи; D124 разделяет
wall-clock и monotonic **на уровне типов** — это уже сильнее Go/JS/Java; `Time` — ambient suspend-эффект D64 →
детерминизм в тестах через handler-подмену, что **строго лучше** Clock-DI у Java/Kotlin и global-monkey-patch у
JS/tokio, см. §1a). Но есть **7 дефектов поверхности + 1 критический пробел корректности**:

1. **`Time.now() -> int` — нетипизированный провод (`[M-time-now-schema-mismatch]`).** Codegen-схема возвращает
   `nova_int`, а stdlib/handler'ы объявляют `now() -> Timestamp`. `Time.now().minus(other)` роутится по **int-receiver
   path** → ломается method-dispatch. Workaround сейчас — руками `Timestamp.from_unix_millis(Time.now())`.
2. **`Monotonic.now()` — compiler-builtin, а не `.nv`.** Чтобы обойти mismatch, монотонные часы захардкожены **в двух
   dispatch-сайтах**: [emit_c.rs:24553-24554](../../compiler-codegen/src/codegen/emit_c.rs#L24553) (Member-form) +
   [emit_c.rs:27131-27132](../../compiler-codegen/src/codegen/emit_c.rs#L27131) (Path-form) → `nova_monotonic_now_record()`
   ([channels.h:1428-1431](../../compiler-codegen/nova_rt/channels.h#L1428)). Нарушает §3-правило «брать из `.nv`»
   ([[feedback-maximize-nv-sourcing]]) и делает их **немокабельными** (`[M-monotonic-mock-support]`).
3. **ПЯТЬ расходящихся источников одной схемы** (аудит уточнил — было сказано «4»): (a) prelude-decl
   [effects.nv:137-140](../../std/prelude/effects.nv#L137) (`sleep(ms int)`, `now()->int`); (b) codegen-schema
   [emit_c.rs:2297-2319](../../compiler-codegen/src/codegen/emit_c.rs#L2297) (`sleep`/`now`/`now_monotonic` + 5 timer-счётчиков);
   (c) C-vtable [effects.h:863-869](../../compiler-codegen/nova_rt/effects.h#L863) (ctx/sleep/now/`now_ms`/`now_ns`);
   (d) handler-литералы [handlers.nv:180-221](../../std/testing/handlers.nv#L180) (`now()->Timestamp`, `now_ms`/`now_ns`,
   `sleep(d Duration)`); (e) **закомментированная** decl [duration.nv:541-546](../../std/time/duration.nv#L541). Менять = править 5 мест.
4. **`sleep(ms int)` vs `sleep(d Duration)`.** prelude берёт сырой `int` ms; handler'ы/usage — `Duration`. Bridge
   `[M-handler-duration-schema-mismatch]` частично закрыт (annotation-мост), но канон в decl всё ещё `int`.
5. **Единица неоднозначна** (`now()->int` — ms или ns?). Решено: **ns везде** (§3.0-Q5).
6. **`Time.now()→Monotonic` миграция заморожена** (`[M-monotonic-migration-deferred]`, ≈9 сайтов timing-логики, §6).
7. **`now_ms`/`now_ns`** живут только в vtable+handler'ах (НЕ в codegen-schema) — рудимент int-провода (убрать, §3).
8. **🔴 КРИТИЧНО — Duration-арифметика молча переполняется.** ВСЕ операторы [duration.nv:264-323](../../std/time/duration.nv#L264)
   (`@plus`/`@minus`/`@neg`/`@times`/`@div`) — сырые **unchecked i64**, two's-complement **WRAP** на ±292 годах. Это
   ровно Go-ловушка («the trap to avoid»), а Rust/Java/Kotlin/Temporal все детектят overflow. **В текущем плане это
   полностью отсутствовало** → добавлено фазой Ф.1c + D317. Без этого критерий «без упрощений» недостижим.

Гражданское время (Date/DateTime/TimeZone/Period, ISO-8601 format/parse) — **вне scope**, под-план
[179.1](179.1-civil-time.md).

## 1a. Где Nova УЖЕ лучше peers (зафиксировать в доке как differentiators)

- **Clock-injection через алгебраический эффект-handler — строго лучше всех 5 языков.** Java/Kotlin `Clock`-DI —
  виральна и **молча падает на real-clock**, если хоть один `now()` забыли пробросить (нет помощи компилятора); JS
  `@sinonjs/fake-timers` и tokio `pause` — глобальный monkey-patch (нужен cleanup, concurrency-unsafe, tokio — async-only);
  Go 1.25 `synctest` — runtime-«пузырь». Nova-handler **лексически скоупнут, композируется, виден в effect-row типа,
  работает sync+async, без cleanup**. **НО** заявка верна только после Ф.3 (роутинг `monotonic()` через эффект).
- **Compile-time wall-vs-monotonic separation (D124)** уже бьёт Go (один `Time` со скрытым mode-bit + runtime-fallback),
  JS-legacy (оба — голый `number`), Java (`nanoTime` — типонезависимый `long`). Наравне с Rust/Kotlin.
- **Единый `Time`-эффект на wall+monotonic+sleep** решает то, что Kotlin **не может**: у Kotlin ТРИ несвязанных
  clock-авторитета (`Clock`/`TimeSource`/`TestCoroutineScheduler`), которые рассинхронятся. У Nova — один.
- **`sleep_until(Monotonic)`-only** (нет `sleep_until(Timestamp)`) — дизайн, который Java сделала **неправильно**
  (`parkUntil` на wall → JDK-8146730: hibernation/NTP-скачок ломает таймеры), а Go/JS/Kotlin вовсе не имеют.

## 2. Текущая схема (как есть, факты с file:line)

| Источник | wall | sleep | monotonic | extra | file:line |
|---|---|---|---|---|---|
| prelude decl | `now()->int` | `sleep(ms int)` | — | — | [effects.nv:138-139](../../std/prelude/effects.nv#L138) |
| codegen schema | `now->nova_int` | `sleep(nova_int)->nova_unit` | `now_monotonic->nova_int` | 5×timer-счётчик | [emit_c.rs:2297-2319](../../compiler-codegen/src/codegen/emit_c.rs#L2297) |
| C-vtable | `now` | `sleep` | — | `now_ms`/`now_ns` | [effects.h:863-869](../../compiler-codegen/nova_rt/effects.h#L863) |
| handlers | `now()=>Timestamp` | `sleep(d Duration)` | — | `now_ms`/`now_ns` | [handlers.nv:180-221](../../std/testing/handlers.nv#L180) |
| commented-out | `now()->Timestamp` | `sleep(d Duration)` | — | `now_ms`/`now_ns` | [duration.nv:541-546](../../std/time/duration.nv#L541) |
| stdlib типы | `Timestamp{nanos i64}` (heap) | — | `Monotonic{nanos i64}` (heap, builtin `now()`) | — | [duration.nv](../../std/time/duration.nv) |

Builtin monotonic dispatch: [emit_c.rs:24553-24554](../../compiler-codegen/src/codegen/emit_c.rs#L24553) (Member) +
[27131-27132](../../compiler-codegen/src/codegen/emit_c.rs#L27131) (Path) → `nova_monotonic_now_record()` heap-alloc.
Runtime-часы: `uv_hrtime()` ([fibers.h:2260/2276](../../compiler-codegen/nova_rt/fibers.h#L2260)), sleep
([fibers.h:2811](../../compiler-codegen/nova_rt/fibers.h#L2811)). 5 observability-счётчиков
(`timer_alloc_total/active/fired/cancelled/longest_pending_ms`) — **не «время», а timer-runtime-интроспекция** (Plan 65 Ф.11).

## 3. Новая схема (типизированный эффект; один источник)

**Принцип.** `Time` — **внутренний плумбинг-эффект** (как `TcpNet`/`AddrNet`, [net/effect.nv §21-40](../../std/net/effect.nv#L21)):
user-код его **не вызывает напрямую**, а ходит через type-методы. Эффект отдаёт **типизированные value-записи**, не int;
единица — **наносекунды**; схема живёт в **одном** месте (`.nv`-decl), codegen её **читает**; default-handler = тонкие
**`extern "C" fn`**-примитивы — module-private C-символы в `nova_rt` по литеральному имени (как `std/net/ffi.nv`).
**Обоснование (D282):** keyword выбирает только имя эмитируемого символа + проверку C-нативных типов — `extern "nova"`→
`nova_fn_<name>`, `extern "C"`→литеральное `<name>` (типы обязаны быть C-нативными); **никакой** suspend/GC/effect-семантики
в keyword нет. Все 3 хука — C-нативные скаляры (`int`/`()`), Nova-типизация (`Timestamp`/`Monotonic`/`Duration`) живёт в
`.nv`-обёртке (handler), не в externe → **`extern "C"`** (как net). `extern "nova"` понадобился бы только если хук
принимал/возвращал Nova-тип (ср. `sync.nv` `Mutex @try_lock_for(Duration)`). Impl в C (не выдумывать, [[feedback-maximize-nv-sourcing]] §3).

**Эффект (плумбинг — юзер не трогает; опы названы по возвращаемому типу, как `AddrNet.loopback`/`v4`):**
```nova
type Time effect {
    timestamp() -> Timestamp     // wall-clock read (Unix epoch ns); может прыгать (NTP/DST)
    monotonic() -> Monotonic     // монотонные часы (ns); non-regression contract D318
    sleep(d Duration) -> ()      // suspend текущего fiber на >= d (D64, cancellable); d<=0 => немедленно
}
```
**TimerMetrics (отдельный read-only surface, Mem-style) — 5 счётчиков ВЫНЕСЕНЫ из `Time`** (решение Q1): они —
интроспекция timer-runtime (Plan 66 territory), не «время», read-only (не suspend). Иначе test-handler'ы вынуждены
стабить 5 бессмысленных опов.

**User-facing surface (на типах + free-fn) — только это видит юзер:**
```nova
Timestamp.now()  => Time.timestamp()        // .nv-сахар
Monotonic.now()  => Time.monotonic()        // .nv-сахар (из compiler-builtin → .nv, Ф.3)
fn sleep(d Duration) Time => Time.sleep(d)  // free, prelude-export (метод-формы d.sleep() НЕТ)
fn sleep_until(deadline Monotonic) Time     // монотонный дедлайн (tokio-паритет; MVP-обёртка, Q3)
```
- `sleep_until` — **только `Monotonic`** (дедлайн иммунен к NTP/DST). Wall-абсолютный сон — **явно** `sleep(ts.time_until())`
  (footgun виден на call-site); `sleep_until(Timestamp)` **не вводим** (`E_SLEEP_UNTIL_WALL` с fix-it на `sleep(ts.time_until())`).
- `sleep` — единственный оп, который юзер зовёт «как есть»; free-обёртка прячет эффект (как net), `Time` виден в сигнатуре.

| Операция | Было | Стало |
|---|---|---|
| wall | `now()->int` | эффект `timestamp()->Timestamp` + сахар `Timestamp.now()` |
| monotonic | builtin (i64), 2 dispatch-сайта | эффект `monotonic()->Monotonic` + сахар `Monotonic.now()` (builtin удалён) |
| sleep | `sleep(ms int)` | эффект `sleep(d Duration)` + free `sleep`/`sleep_until` |
| `now_ms`/`now_ns` | vtable+handler-only | **удалить** (= `Timestamp.now().as_unix_millis()`/`…nanos()`) |
| 5 счётчиков | в `Time` | **вынести** в `TimerMetrics` |
| единица | ms/ns дрейф | **ns** канон |

**ABI-ключ.** `Duration`/`Timestamp`/`Monotonic` = `{ ro nanos i64 }`, но **сейчас heap reference-records** (D215:
`{}` = heap; C — `Nova_Duration*`, [duration.c:1965](../../std/time/duration.c#L1965)). Поэтому Ф.2 предваряется
**Ф.1b — миграцией в `value`-records** (прецедент Plan 165): (a) stack/zero-GC; (b) каждый тип = ровно один i64 →
**узкий single-i64 scalar-bridge через границу эффекта provably sound** (Q2), без блокировки на полный 172.4.

## 3.0. Закрытые решения (бывшие открытые вопросы — РЕШЕНЫ, не «Ф.0 решит»)

| # | Вопрос | РЕШЕНИЕ | Обоснование |
|---|---|---|---|
| Q1 | 5 observability-счётчиков | **Вынести в отдельный `TimerMetrics`-surface** (read-only), убрать из `Time` (Ф.1) | Минимальный плумбинг-эффект; счётчики — Plan 66 territory; не заставлять handler'ы стабить |
| Q2 | record-через-границу | **Узкий single-i64 scalar-bridge поверх Ф.1b**, НЕ блокироваться на 172.4 | Каждый тип = 1×i64 → bridge sound by construction; forward-compatible (172.4 субсумирует) |
| Q3 | `sleep_until` MVP/later | **MVP** (Ф.3): обёртка `sleep(deadline - Monotonic.now())` (оператор `-` = `@minus(Monotonic)`, saturate-to-zero D318 → прошлый дедлайн = немедленно); true re-arm timer → Plan 66 | ~5 строк, drift-free семантика, tokio-паритет которого нет у Go/JS/Kotlin/Java |
| Q4 | `@elapsed_since` vs `@minus(Monotonic)` | **Убрать `@elapsed_since`**, дать overload `@minus(Monotonic)->Duration` + `checked_duration_since(other)->Option[Duration]` | Симметрия с Timestamp; Go-стиль; checked — escape-hatch Rust |
| Q5 | единица | **ns везде** (storage + wire); `now_ms`/`now_ns`-опы убрать | Уже storage-unit; ns = precision-floor uv_hrtime/Rust/Java/Temporal |
| Q6 | метод-форма `d.sleep()` | **Нет**, только free `sleep(d)` | Go/Rust — free fn; «один очевидный способ» |
| Q7 | `Duration.from_days/weeks` | **Оставить** как exact `N×86400s`, задокументировать «не календарный день → `Period` 179.1» | Математически точны, полезны для интервалов; удаление ломает API без выигрыша |
| Q8 | имена эффект-опов | `timestamp()`/`monotonic()`/`sleep()` (по возвращаемому типу) | Симметрия `AddrNet.loopback/v4`; `.now()` — ergonomic-сахар на типе |
| Q9 | **overflow-политика Duration** | **Trap-default операторы + `checked_*`(→Option) + `saturating_*`** (3-tier, Rust/Java-урок) | Go-ловушка (silent wrap) — недопустима; см. §3b/D317 |
| Q10 | **monotonic регресс** | **Saturate-to-zero** на `@minus`/`elapsed` + `checked_duration_since`→`None`; **без global-lock** (урок Rust 1.60) | HW/VM/OS-баг (JDK-6458294); не паниковать, не лочить hot-path |
| Q11 | **signedness** | **Signed i64 ns, ±292y, задокументировать границу** | Уже signed (`@neg`, negative `@minus`); бьёт Rust unsigned-forces-fallible |
| Q12 | **формат `@display`** | **ASCII** (`"us"` не `"µs"`) human auto-scale + отдельная **machine ISO-8601** форма | non-ASCII µs (U+00B5) ломает byte-exact golden-тесты (nova_tests — byte-baseline!) |
| Q13 | Monotonic сериализация | **Запрещена by contract** (process-local); сериализуется только `Timestamp` | Go течёт `m=…` в `String()` (footgun); D318 |

## 3a. Методы `Duration`/`Timestamp`/`Monotonic`: есть → после рерайта

**Инвариант:** существующий surface сохраняется (рерайт *чинит* int-провод), кроме осознанного `@elapsed_since`→`@minus`.
Меняется представление (Ф.1b heap→value), провод (Ф.2 int→typed); **добавляются** overflow-safe варианты (Ф.1c),
`@display`/`@debug` (D237), `.now()`-сахар, `checked_*`.

**`Duration`** (`#stable 0.1`, [duration.nv:52](../../std/time/duration.nv#L52)):

| Метод | Было | После |
|---|---|---|
| consts ZERO/SECOND/MINUTE/HOUR; `from_*`/`as_*`/`is_*`/`parts` | работают | без изменений (но consts — value-const-evaluable после Ф.1b) |
| `@plus`/`@minus`/`@neg`/`@times(i64\|f64)`/`@div(i64\|f64)`/`@abs` | **unchecked i64 wrap** 🔴 | **trap-on-overflow** (Ф.1c) |
| `checked_add/sub/mul/div(...)->Option[Duration]` | — | **NEW** (Ф.1c) |
| `saturating_add/sub/mul(...)->Duration` | — | **NEW** (Ф.1c, clamp к ±MAX) |
| `try_from_secs_f64`/`@times(f64)`/`@div(f64)` NaN/inf | сырой cast → мусор | **NEW try_*** → `Option`/trap на NaN/inf/overflow (Ф.1c) |
| `@compare` | работает | без изменений |
| `@display`/`@debug` (sink `mut w Write`) | — | **NEW** D237; `@display` = ASCII auto-scale (`"2s"`/`"500ns"`/`"us"`); `@debug` диагностика; машинная ISO-8601 форма отдельно |
| `@into()->str`/`@into_human()` | µs (U+00B5) 🔴 | `@into` делегирует в `@display` (ASCII); `@into_human` остаётся extra |

**`Timestamp`** (`#stable 0.1`): + `Timestamp.now() => Time.timestamp()` (**NEW** сахар); `@plus(Duration)`/`@minus(Duration)`
→ **saturate at boundary** + `checked_add/sub->Option[Timestamp]` (**NEW** Ф.1c); `@minus(Timestamp)->Duration` (есть);
`@is_past`/`@time_until`/`@elapsed` — **начинают работать** (Ф.2); `@display`/`@debug` (**NEW**; full-datetime ждёт 179.1,
до этого `@debug` = `Timestamp(unix_ns=…)`); сериализуем (только этот тип).

**`Monotonic`** (`#stable 0.6`): `Monotonic.now()` builtin→`.nv` `=> Time.monotonic()` (мокабелен, Ф.3); `@as_nanos`
(есть, escape-hatch); `@plus(Duration)`/`@minus(Duration)`->Monotonic + saturate/checked (Ф.1c); **NEW** `@minus(Monotonic)->Duration`
(saturate-to-zero на регресс, D318) + `checked_duration_since(other)->Option[Duration]` (None на регресс); `@elapsed_since`
**УДАЛИТЬ**; `@compare`; `@display`/`@debug` (`@debug` = offset `Monotonic(+1.234s)`, **не дата**); **non-serializable**;
`Monotonic.from_*` — **НЕ вводить** (opaque, как Rust `Instant`); ⛔ `Monotonic ± Timestamp`/`as_unix_*` → compile-error (D124).

## 3b. Арифметика и overflow (D317 — production-grade, паритет Rust/Java, бьёт Go)

- **3-tier дисциплина** (Rust-урок): (1) операторы `+`/`-`/`*`/`/`/унарный `-` → **trap-on-overflow** в debug И release
  (никогда не молчаливый wrap — Go-ловушка); (2) `checked_*` → `Option[T]` (None на overflow); (3) `saturating_*` → clamp.
- **Граница** = ±(2⁶³−1) ns ≈ **±292 года** (i64), задокументировать как контракт.
- **Асимметрия two's-complement:** `@abs(i64::MIN ns)` НЕ должен быть UB → saturate к `i64::MAX` (Go off-by-1ns) ИЛИ `checked`.
- **`@div(0)`** → trap/`E`; **`@neg(i64::MIN)`** → saturate.
- **Граничная арифметика инстантов:** `Timestamp`/`Monotonic` `@plus(Duration)`/`@minus(Duration)` → **saturate at boundary**
  (зеркало Go `addSec`-clamp) + `checked_*->Option`. `Timestamp - Timestamp` / `Monotonic - Monotonic` → саturating diff.
- **f64-конверсии** (`@times(f64)`/`@div(f64)`/`from_secs_f64`): NaN/inf/overflow → `try_*`→`Option`/trap (Rust паникует);
  не молчаливый мусор-cast. Текущий код [duration.nv:287/298](../../std/time/duration.nv#L287) предупреждает про FP-округление, но не про NaN/inf.

## 3c. Monotonic non-regression contract (D318)

`monotonic()` читает `uv_hrtime()` (OS CLOCK_MONOTONIC/QPC). Контракт при кажущемся регрессе (later mark < earlier):
**`@minus(Monotonic)` и `elapsed` SATURATE-to-ZERO** (никогда negative, никогда panic, **без global-lock** — урок Rust
1.60-saga); `checked_duration_since(other)->Option[Duration]` → `None` на регрессе (для correctness-sensitive). Зафиксировать
как **стабильный** контракт (чтоб не флип-флопил как у Rust). `Monotonic` **non-serializable** (process-local).

## 4. Фазы (mandatory-now vs later)

**Dep-chain:** Ф.0 → Ф.1 → Ф.1b → {Ф.1c ∥ Ф.2} → Ф.3 → Ф.4 → Ф.5 → Ф.6. (Ф.1c и Ф.2 оба зависят от Ф.1b, но
независимы между собой — параллелятся двумя агентами.) **Коммит после каждой фазы** (§10).

- **Ф.0 — gate (без кода).** Написать черновики **D316/D317/D318** (содержание §3.0/§3b/§3c) + amend-планы D124/D237/prelude-decl.
  Все Q уже закрыты (§3.0) — Ф.0 только оформляет в D-блоки и проходит spec-review. **GATE:** D-блоки ревью до кода (§5 spec-first).
- **Ф.1 — единый источник схемы (без смены поведения).** Механизм: codegen **читает** схему `Time` из `.nv`-decl
  (коорд. 172.1 U.1/U.2) вместо хардкода [emit_c.rs:2297](../../compiler-codegen/src/codegen/emit_c.rs#L2297); вынести 5
  счётчиков в `TimerMetrics`; удалить закомментированный 5-й источник [duration.nv:541-546](../../std/time/duration.nv#L541);
  выровнять vtable [effects.h:863](../../compiler-codegen/nova_rt/effects.h#L863). **Содержимое схемы пока НЕ меняем**
  (остаётся int-провод) → поведение не меняется; типизация — Ф.2. DEP: нет (низший риск, де-рискует остальное).
- **Ф.1b — value-migration (enumerated checklist, НЕ «проще»).** `Duration`/`Timestamp`/`Monotonic` `{}`→`value`. По каждому
  риск-сайту аудита — шаг + верификация: (1) 3 типа stack-alloc в 26 методах (ABI); (2) **value-const** ZERO/SECOND/MINUTE/HOUR
  [duration.nv:58-70](../../std/time/duration.nv#L58) + EPOCH [:430](../../std/time/duration.nv#L430) — const-evaluable; (3)
  `DurationParts` (7 полей) **остаётся heap** (display-helper); (4) **D290 generic-forward-decl**: `Option[Duration]`/`Vec[Timestamp]`/
  `Result[Monotonic,E]` — complete struct ДО инстанциирования → Plan 91.12 `/*__VALUE_RECORD_DEFS__*/` перед `/*__NOVAOPT_TYPEDEFS__*/`
  ([emit_c.rs:921](../../compiler-codegen/src/codegen/emit_c.rs#L921)); (5) монотоник builtin heap-alloc [emit_c.rs:24553](../../compiler-codegen/src/codegen/emit_c.rs#L24553)
  → stack-init; (6) cross-module handler'ы [handlers.nv](../../std/testing/handlers.nv); инфра `AllocKind::Value`/`emit_value_record_type`/`NovaValue_`
  ([emit_c.rs:2441](../../compiler-codegen/src/codegen/emit_c.rs#L2441)). DEP: Ф.1. GATE для scalar-bridge.
- **Ф.1c — overflow-safe арифметика (NEW, mandatory).** Реализовать §3b/D317: trap-операторы + `checked_*`/`saturating_*`
  на Duration; boundary-saturate + `checked_*` на Timestamp/Monotonic±Duration; `@abs(i64::MIN)`; `@div(0)`; f64 NaN/inf-policy.
  DEP: Ф.1b (тела всё равно переписываются). **Здесь Nova достигает паритета Rust/Java/Kotlin и обходит Go.**
- **Ф.2 — типизированный провод (retire int-wire).** Изменить `.nv`-decl `Time` на typed-surface (`timestamp()->Timestamp`/
  `monotonic()->Monotonic`/`sleep(d Duration)`); единый источник (Ф.1) пропагирует во все места; узкий scalar-bridge (Q2)
  для записей через границу. Убрать `now()->int`/`now_ms`/`now_ns`. **Закрывает `[M-time-now-schema-mismatch]`.** DEP: Ф.1b.
- **Ф.3 — user-facing surface.** (a) сахар `Timestamp.now()=>Time.timestamp()`, `Monotonic.now()=>Time.monotonic()` —
  **удалить builtin-dispatch [emit_c.rs:24553-24554](../../compiler-codegen/src/codegen/emit_c.rs#L24553) И [27131-27132](../../compiler-codegen/src/codegen/emit_c.rs#L27131)**
  (НЕ строку 2312 — это schema-reg), runtime-примитив теперь зовётся через default-handler → **закрывает `[M-monotonic-mock-support]`**;
  (b) free `sleep(d Duration) Time` (prelude-export) + `sleep_until(deadline Monotonic) Time` (MVP-обёртка); (c) `@elapsed_since`
  → overload `@minus(Monotonic)->Duration` + `checked_duration_since` (D318); **pos-test доказывает `m2 - m1` диспатчится в
  `@minus(Monotonic)`, не `@minus(Duration)`**; если мис-диспатч — **фикс резолюции** (коорд. 172.1, verifiable fixture);
  (d) `@display`(ASCII)/`@debug` + machine-форма (D237). DEP: Ф.2.
- **Ф.4 — sleep-канон + unit + семантика.** `sleep(ms int)`→`sleep(d Duration)` в decl/handler'ах; `sleep(d<=0)`→немедленно
  (Go/tokio); задокументировать granularity (uv-timer ~1ms) и «sleep гарантирует ≥ d»; days/weeks-заметку; финализировать
  `[M-handler-duration-schema-mismatch]`. DEP: Ф.3.
- **Ф.5 — handlers + auto-advance + миграция + M:N-контракт.** (a) default-handler: module-private **`extern "C" fn`**
  скаляр-примитивы `time_wall_now_ns() -> int` / `time_monotonic_now_ns() -> int` / `time_sleep_ns(ns int) -> ()`
  — литеральные C-символы в `nova_rt`; **именование `<resource>_<action>` без Nova-префикса** (D282/net-конвенция,
  `02-types.md:12962`; лидирующий `_` у глобального C-символа зарезервирован C-стандартом); D282 rule 2: все типы C-нативные.
  Nova-типизация — в `.nv`-обёртке
  `real_time() -> Effect[Time]` (часы оборачиваются в `Timestamp`/`Monotonic`, `Duration.nanos`→ns). [[feedback-maximize-nv-sourcing]] §3 (impl в C);
  (b) `fixed`/`mut_clock` под новую typed-схему; (c) **auto-advance virtual clock** (tokio/Kotlin/Go-synctest killer-feature):
  под paused-clock, когда все фибры в scope durably-blocked на `sleep`, handler **авто-продвигает** время к ближайшему дедлайну
  и будит спящего (hook в [fibers.h](../../compiler-codegen/nova_rt/fibers.h) park/wake) — если велик, MVP = explicit `advance(d)`
  отпускающий due-sleepers + followup на auto-idle; (d) мигрировать ≈9 timing-сайтов (§6) на `Monotonic.now()` →
  **закрывает `[M-monotonic-migration-deferred]`**; (e) **M:N thread-safety контракт**: default-handler stateless/thread-safe;
  stateful `mut_clock` — virtual-clock-тесты под `NOVA_MAXPROCS=1`/AUTOARM-паттерн ([[reference-mn-race-case-study]]); neg-нота. DEP: Ф.4.
- **Ф.6 — тесты + per-OS + spec/docs.** §7 pos+neg; per-OS monotonicity (`[M-monotonic-per-os-isolated-tests]`, опц. dedicated
  [nova_rt/time.c](../../compiler-codegen/nova_rt)); amend D-блоки; новый/обновлённый `docs/time.md` (модель + «было→стало» +
  таблица «Nova vs Go/Rust/TS/Kotlin/Java» + differentiators §1a). DEP: all.

**DEFERRABLE-LATER (явно НЕ в 179):** true re-arm deadline-timer / `tick_every` → **Plan 66** (разблокируется); полный
multi-field 172.4 (узкий bridge субсумируется); гражданское время → **179.1**; auto-idle-advance (если Ф.5 даёт только explicit `advance`).

## 5. Spec / D / Q / docs

- **NEW D316** — «`Time`-эффект: typed plumbing-surface (`timestamp`/`monotonic`/`sleep`) + единый источник (codegen
  читает из `.nv`) + `TimerMetrics`-split + ns-канон + non-serializable Monotonic».
- **NEW D317** — «Duration/instant overflow-policy: trap-default + `checked_*`/`saturating_*`; ±292y граница; `@abs`/`@div(0)`/
  f64-NaN/inf; boundary-saturate Timestamp/Monotonic».
- **NEW D318** — «Monotonic non-regression contract: saturate-to-zero + `checked_duration_since`; без global-lock».
- **amend D124** — оба часовых типа типизированно из эффекта; `Monotonic.now()` = `.nv`-обёртка `=> Time.monotonic()`;
  `@elapsed_since`→`@minus(Monotonic)`; Monotonic non-serializable.
- **amend D237** ([protocols.nv:334/358/405](../../std/prelude/protocols.nv#L334)) — `Duration`/`Timestamp`/`Monotonic` реализуют
  `@display`/`@debug` (sink, ASCII); `@into()` (D73, `Into[str]`) делегирует в `@display`. *(D73 = Into[str], НЕ Display — не путать.)*
- **amend prelude `Time`-decl** (D11/D14/D62, [04-effects.md]) — плумбинг-эффект, typed-опы, user ходит через типы.
- **error-index (NEW codes):** `E_SLEEP_UNTIL_WALL` (`sleep_until(Timestamp)` запрещён, fix-it → `sleep(ts.time_until())`);
  Duration-overflow trap-код; (если вводим) `E_DURATION_F64_NONFINITE`. **Realtime:** реальный код = `E_REALTIME_SYNC_PARK`
  ([emit_c.rs:24449](../../compiler-codegen/src/codegen/emit_c.rs#L24449)), НЕ `E_EFFECT_REALTIME_VIOLATION` (только в комментах);
  D64 живёт в [113-realtime-blocking-attribute-only.md](113-realtime-blocking-attribute-only.md), не в `decisions/`. **Ф.6 сначала
  ВЕРИФИЦИРУЕТ**, даёт ли `Time.sleep` внутри `realtime{}` диагностику сегодня; если нет — добавить check+код (не assume).
- **docs/** — новый `docs/time.md` (по образцу [docs/strings-internals.md](../strings-internals.md)); таблица «было→стало»;
  differentiators §1a; убрать упоминания int-провода как «текущего». Q-файл: занести закрытые Q3.0 в open-questions как RESOLVED.

## 6. Миграция (§7 compiler-conventions) — blast-radius + точные команды

**Blast-radius (измерено аудитом):** **447** вызовов `Time.now()`/`Time.sleep()` в `nova_tests/` (30+ файлов); **26**
методов в `duration.nv` через `Time`; `Monotonic.now()` — **11** вхождений в тестах; 2 handler-фабрики; codegen-схема
[emit_c.rs:2297-2319](../../compiler-codegen/src/codegen/emit_c.rs#L2297). `uv_hrtime()` в sync_*.h (barrier/condvar/countdown/semaphore)
для deadline-mgmt. **Сначала пере-измерить grep'ом** (числа — снимок), переписать в ТОМ ЖЕ изменении.

**≈9 timing-сайтов на `Monotonic.now()` (Ф.5.d, enumerated):** `duration.nv:516` (`is_past`), `:522` (`time_until`),
`:528` (`elapsed`), `:681-685` (`measure[T]` ×2), `:692` (`deadline_in`); тесты: `cancel_cycle_linked_tokens.nv`,
`cancel_during_natural_fire.nv`, `condvar_wait_cancel.nv`, `sleep_real_clock.nv`, `cancel_latency_bench.nv`, rate_limiter.
**Перед Ф.5 — `grep -rn "Time.now()" std/ nova_tests/` для полного списка** (часть зависят от поведения, мигрировать осознанно).

**Команды верификации (Bash/PS cap = 10мин, [[project-bash-timeout-10min-max]] — дробить):**
- single-fixture: `compiler-codegen/target/debug/nova-codegen test-build nova_tests/time179/<f>.nv --toolchain clang --keep-artifacts`
- targeted: `nova-cli/target/release/nova test --filter time179` (+ `--filter plan65`, `--filter concurrency` батчами)
- mass compile-errors → **per-file loop** (`nova check FILE` → fix → re-check), НЕ full-regress-в-loop ([[feedback-test-fix-per-file-loop]]).
- codegen-верификация = kill-switch baseline на **том же** бинаре ([[feedback-codegen-dce-verification]]), не sibling/stale.
- **Пересобрать `nova-cli` после правок `.nv`** (time/sync `.nv` вшиты через `include_str!` — stale-бинарь не увидит).

## 7. Тесты (pos + neg; раскладка + EXPECT-маркеры)

**Раскладка:** `nova_tests/time179/` (pos, standalone с `fn main`/`EXPECT:`), `nova_tests/time179/neg/` (`module neg.<name>`,
`// EXPECT_COMPILE_ERROR: <substr>`). Раннер классифицирует по маркеру, не по папке ([[feedback-test-conventions-strict]]).
Per-fix verify = targeted fixture; full regress — в конце фазы.

**pos:** `Timestamp.now().elapsed()/.minus()/.time_until()` без обёртки (роутинг на Timestamp-методы); `m2 - m1` (overload
`Monotonic @minus(Monotonic)` — assert диспатч, не `@minus(Duration)`); `checked_duration_since` (Some/None); `sleep(Duration)`
+ `sleep_until(Monotonic)` (drift-free цикл) + `sleep(ZERO)`→немедленно; `${d}`/`${d:?}` (`@display`/`@debug`); **mock через
`fixed`/`mut_clock` — в т.ч. перехват `Monotonic.now()`** (раньше невозможно); **auto-advance**: `sleep(10.minutes)` под
paused-clock резолвится мгновенно; `with Time = …` детерминизм; checked_/saturating_ Duration-арифметика (Some/None/clamp).

**neg (`EXPECT_COMPILE_ERROR`):** `Monotonic ± Timestamp` / `Monotonic.as_unix_*`/`from_unix_*` → нет метода (D124);
`sleep_until(Timestamp)` → `E_SLEEP_UNTIL_WALL` (+ fix-it); `Monotonic.from_*` → нет; (после верификации §5) `Time.sleep`
внутри `realtime{}` → реальный код. **EXPECT-RUNTIME/трапы:** Duration-overflow оператор → trap (`EXPECT_RUNTIME_PANIC`/exit);
`@div(0)`; f64 NaN→Duration.

**byte-exact / контрактные:** **`@display` без байтов > 0x7F** (ASCII `"us"` не `"µs"`) — отдельный assert; Duration `"0s"`
zero-литерал; trailing-fractional-zeros обрезаются; **Monotonic `@debug` = offset, не дата**; **Monotonic не сериализуется**
(нет derive-пути, экспонирующего `.nanos` как portable); **value-const** ZERO/SECOND/MINUTE/HOUR/EPOCH компилируются как
const после Ф.1b.

**per-OS:** monotonicity (`monotonic()` не убывает) Win+Linux; wall vs monotonic не путаются. Под `NOVA_MAXPROCS=1`/AUTOARM
для timing-фикстур ([[reference-mn-race-case-study]]).

## 8. Критерии приёмки

0. **🔴 ОБЯЗАТЕЛЬНО: «без упрощений, как для прода».** Ни одной молча-переполняющейся операции, ни одного «решим потом»
   на критическом пути, ни одного untested поведения; каждая behavior-change закрыта pos+neg-фикстурой + аргументом звучности.
1. `Time`-эффект — typed плумбинг: `timestamp()->Timestamp`/`monotonic()->Monotonic`/`sleep(Duration)`; int-провод
   (`now()->int`/`now_ms`/`now_ns`/`now_monotonic`) ретайрнут; 5 счётчиков в `TimerMetrics`.
2. Схема `Time` — **один** источник (codegen читает из `.nv`); 5-й закомментированный источник удалён.
3. User-facing: `Timestamp.now()`/`Monotonic.now()` (сахар, **оба builtin-dispatch-сайта 24554+27132 удалены**, мокабельны)
   + free `sleep` + `sleep_until(Monotonic)`; `@display`(ASCII)/`@debug` на трёх типах; `@elapsed_since` убран → `@minus(Monotonic)`
   + `checked_duration_since`.
4. **🔴 Overflow-safe (D317):** ни один Duration-оператор не wrap'ает молча (trap); есть `checked_*`/`saturating_*`;
   Timestamp/Monotonic±Duration saturate at boundary; `@abs(i64::MIN)`/`@div(0)`/f64-NaN/inf обработаны; ±292y задокументировано.
5. **Monotonic non-regression (D318):** `@minus`/`elapsed` saturate-to-zero; `checked_duration_since`→None на регрессе; без
   global-lock; non-serializable.
6. **`@display` byte-exact ASCII** (нет байтов >0x7F); отдельная machine-форма round-trip'ит.
7. `Duration`/`Timestamp`/`Monotonic` — `value`-records (стек); value-const'ы компилируются; `Monotonic` без `from_*`.
8. ≈9 timing-сайтов мигрированы на `Monotonic`; mock перехватывает `Monotonic.now()`; auto-advance (или explicit `advance` + followup).
9. Закрыты `[M-time-now-schema-mismatch]`/`[M-monotonic-mock-support]`/`[M-monotonic-migration-deferred]`; финализированы
   `[M-handler-duration-schema-mismatch]`/`[M-monotonic-per-os-isolated-tests]`.
10. **Полный регресс зелёный** (батчами <10мин; sample-policy [[feedback-large-tests-stored-not-in-regress]]); 447 call-сайтов
    компилируются+проходят после смены surface; realtime{}-бан сохранён (верифицированным кодом).
11. spec: D316/317/318 написаны + amend D124/D237/prelude-decl; `docs/time.md`; differentiators §1a зафиксированы.

## 9. Конвенции + координация

§1 (проверки в чекере), §3 (схема/типы из `.nv`, не хардкод — мотив Ф.1/Ф.3), §5 spec-first (D-блоки до кода), §6 (коды
ошибок + error-index), §7 (blast-radius + чистый бинарь), §8 (pos+neg, C-codegen). **Координировать с 172.1** (схема из `.nv`
+ overload-резолюция для `@minus(Monotonic)`) и **172.4** (узкий bridge субсумируется полным value-ABI). `02-types.md`/
effect-схему — не править в одиночку (зона 172). **Разблокирует Plan 66**. После каждой большой задачи — обновить
`project-creation.txt` + discussion-log (nova-private) + `simplifications.md` ([[feedback-update-logs]]); источник истины —
`docs/plans/README.md`/`simplifications.md`/nova-private, **не** external memory ([[feedback-no-external-memory-for-project-state]]).

## 10. Фоновые агенты (если используются при выполнении)

- **НЕ `git stash`** — 4 worktree (`nova`, `www`, `nova-p169-1-2`, `nova-p172`) делят один `.git` → stash/refs/reflog
  repo-global, коллизия с конкурентными агентами ([[feedback-worktree-shared-stash]]). Baseline — **temp-worktree**
  (`git worktree add`) **или** commit+reset, **никогда** stash. Постоянный worktree `nova-p179` (naming `nova-pNN`,
  [[feedback-worktree-naming]]) создать **первой** Bash-командой и самозарегистрироваться; cwd сбрасывается в main →
  **префикс абсолютным путём в каждой команде** ([[feedback_worktree_cwd_clarity]]).
- **Идемпотентность под rate-limit** (workflow-агенты ловят серверный rate-limit и падают mid-run): шаги идемпотентны +
  checkpoint; (a) **коммит после каждой фазы**, маленькими, без amend ([[feedback-commit-per-task]]); (b) `git add` только
  конкретные файлы, никогда `-A`/`.` ([[feedback_git_add_specific]]); (c) `git diff --cached --stat` перед каждым commit
  ([[feedback-verify-index-before-commit]]); (d) **без** `Co-Authored-By` (хук срежет, но не добавлять); (e) full `nova test`
  ~60-90мин > 10-мин cap → батчи <10мин / targeted ([[feedback_targeted_test_per_fix]]).
- **Worktree nova test setup** ([[project-worktree-nova-test-setup]]): env `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → main repo;
  скопировать libuv-submodule из main + удалить `libuv/.git`; net-тесты нужны cwd worktree. **Пересобрать `nova-cli` после
  правок `.nv`** (`include_str!`). Тесты только C-codegen (`nova test`/`test-build`), не интерпретатор ([[feedback-no-interpreter]]).
- **Не выдумывать синтаксис** — `spec/decisions/` + `examples/` ([[feedback_nova_syntax]]). `nova_tests` pass/fail — **не** гейт
  корректности ([[feedback-nova-tests-not-correctness-gate]]): гейт = targeted pos+neg + аргумент звучности.

## 11. Followup

`[M-179-time-system-rework]`. Поглощает `[M-time-now-schema-mismatch]` (Ф.2), `[M-monotonic-mock-support]` (Ф.3),
`[M-monotonic-migration-deferred]` (Ф.5). Гражданское время → [179.1](179.1-civil-time.md). `tick_every` + re-arm
deadline-timer → **Plan 66**. auto-idle-advance virtual clock → followup (если Ф.5 даёт только explicit `advance`).
