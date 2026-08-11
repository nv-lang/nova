<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 175 — Переработка системы времени: типизированный `Time`-эффект (retire int-wire) + overflow-safe Duration + Monotonic из builtin в `.nv` + единый источник схемы

> **Top-level план.** Создан 2026-06-22; production-hardened 2026-06-22 (cross-lang аудит Go/Rust/TS/Kotlin/Java,
> workflow `plan179-harden` — план авторингался под №179, переномерован в 175 при сдвиге std-блока).
> **Ред. 2 — 2026-07-03** (5-агентный аудит + 3-ревьюерная верификация): планка расширена до **7 языков
> (+Zig/Swift)**, ground-truth актуализирован (**file:line = снимок @ cc19478b 2026-07-03; emit_c.rs в зоне
> активного 172.1.2 — пере-grep symbol-якорями перед каждой фазой**), stale-номера старой нумерации вычищены,
> spec_tests-покрытие добавлено (методология 2026-06-28), тест-раскладка приведена к конвенциям, кросс-рефы
> 173-семьи, новые контракты (suspend-семантика Monotonic, infallibility, 2262-горизонт Timestamp).
> **Ф.2-v2 (2026-07-22, ветка `p175-typed-effects`, sonnet, НЕ смёржено в main):** `[M-effect-handler-body-
> record-literal]` ЗАКРЫТ архитектурно (handler-literal capture-механизм → common closure-capture path,
> см. [D431](../../spec/decisions/04-effects.md#d431-default_handlerx--ambient-lazy-default-handler-factory-для-эффектов-plan-175-ф2-v2-2026-07-2122));
> НОВЫЙ generic `#default_handler(X)`-механизм (Time — первый мигрированный эффект). Time typed-schema
> (Duration/Timestamp/Monotonic в опах, relocation в std.time) и ambient-retraction (D62 amend) — **НЕ
> сделаны этой волной** (scalar-bridge + strict-effects-миграция std/examples — оба отдельные окна,
> `[M-175-time-typed-schema-scalar-bridge]`/`[M-175-time-ambient-retraction]` в backlog-followups.md).
> **Статус:** 🚧 IN PROGRESS (ядро закрыто, доводочные пункты остаются TODO) — **Ф.0/Ф.1 ✅; Ф.1b ✅ + Ф.3 ✅
> SHIPPED (option C, 2026-07-04): value-records Duration/Timestamp/Monotonic + полный typed user-surface
> (арифметика/==/compare/neg, `Timestamp.now()`-сахар, `@is_past`/`@time_until`/`@elapsed` int-based,
> `wait_for(Duration)`) поверх НЕизменённого int-wire-эффекта**; **Ф.1c ✅ SHIPPED (2026-07-06): overflow-safe
> арифметика (D317) + Monotonic non-regression (D318)**. **Единицы времени в именах опов ✅ SHIPPED (2026-07-06,
> owner side-task, D316 amend).**
> **Ф.3(a-d) ✅ SHIPPED (2026-07-10, sonnet):** `Monotonic.now()` builtin→`.nv`-сахар (4 emit_c.rs-сайта удалены,
> реальный недостающий кусок — C-vtable-слот `now_monotonic_ns`, НЕ архитектура prelude/std.time — закрывает
> `[M-monotonic-mock-support]`); free `sleep(Duration)`/`sleep_until(Monotonic)`; `@minus(Monotonic)` overload
> (`elapsed_since` сохранён); `@display`/`@debug` (D237) на всех трёх типах + побочный value-record
> interpolation codegen-фикс (`${x}`/`${x:?}` для ЛЮБОГО `value`-record с `@display`/`@debug`, не time-специфичный).
> **Ф.5(d) ✅ SHIPPED:** `measure[T]` мигрирован на `Monotonic` (elapsed-measurement); `deadline_in` намеренно
> НЕ мигрирован (return-type committed к `Timestamp`, D124); `is_past`/`time_until`/`@elapsed` корректно
> остаются `Timestamp`-based (не в списке миграции — старый Ф.5.d line-list устарел).
> **Ф.2 (typed-effect-ops / retire int-wire) — SUPERSEDED (4-й net-zero, 2026-07-10, откачен чисто):**
> prelude⟷std.time coupling решаем, но упёрлось в НОВЫЙ барьер (mock-handler должен конструировать opaque
> `Monotonic`, а codegen не поддерживает anonymous record literal в handler-теле) — см. D316-amend §Ф.2-находка
> + docs/guide/time.md. **Рекомендация зафиксирована:** option C (int-wire + typed-сахар) = корректная итоговая
> архитектура, не временный обход. `[M-time-now-schema-mismatch]` закрыт частично **по конструкции**.
> **with_timeout retraction ✅ SHIPPED (2026-07-10):** `within[T]`/`with_timeout[T]` удалены из
> `std/concurrency/cancellation.nv` (Plan 173 §3a п.4, `[M-174-retract-with-timeout]` CLOSED).
> **Ф.5 auto-idle-advance ✅ ЗАКРЫТ (2026-07-10, ветка `time-tails-175`, merged в main):** deadline-order
> держит под кооперативным `spawn`; armed M:N деградирует безопасно без гарантии порядка (маркер
> `[M-175-vclock-armed-mn-scope-identity]`). Полная гарантия порядка под РЕАЛЬНОЙ concurrent-нагрузкой
> (armed M:N, не кооперативный spawn) вынесена в отдельный **[Plan 189](189-virtual-clock-mn-ordering.md)**
> (`📋 PROPOSED`) — НЕ входит в закрытие этого плана, но ядро 175 больше не блокировано на ней.
> **`[M-rate-limiter-monotonic]` ✅ ЗАКРЫТ (2026-07-11, ветка `time-175`, sonnet):** `std/concurrency/rate_limiter.nv`
> (`TokenBucket`) переведён с wall-clock `Time.now_unix_ms()` на `Monotonic` — блокер (отсутствующий
> `now_monotonic_ns`-vtable-слот) снят ещё Ф.3(a), сама миграция оставалась не сделана; ad-hoc `.max(0)`-clamp
> на регресс заменён нативным D318 saturate-to-zero.
> **Остаётся TODO (не блокеры закрытия ядра):** per-OS dedicated monotonicity test
> (`[M-monotonic-per-os-isolated-tests]`, Priority L, home = simplifications.md секция Plan 65 — явно deferred
> на Plan 58 CI-matrix follow-up, недостижимо в single-OS сессии); полная 755+-сайт nova_tests-миграция
> int-wire→typed (не требуется — wire остаётся int by design, option C).
> **Маркер:** `[M-175-time-system-rework]`. **Запуск:** «**выполни план 175**» (план самодостаточен — вся информация ниже).
> **D-блоки (NEW):** D316 (typed Time-surface + единый источник), D317 (Duration/instant overflow-policy), D318
> (Monotonic non-regression + clock-source contract). Amend: D124, D237, prelude-`Time`-decl. (Резерв подтверждён
> 2026-07-03: D316-D318 свободны — README резервирует D316-D324 за 175/175.1/176; в спеке заняты D315/D325-D328,
> high-water растёт — пере-сверить при spec-шаге.)
> **Координация:** record-через-границу = **узкий single-i64 scalar-bridge** (НЕ блокируемся на 172.4, §3.0-Q2);
> схема-из-`.nv` = коорд. Plan 172.1 (U.1/U.2); effect-vtable storage = Plan 174.4 (регионы файлов дизъюнктны,
> порядок свободный). **Разблокирует:** Plan 66 (`tick_every`); **[173 §3a п.3](173-error-system-unify-harden.md)**
> (`supervised(deadline: Monotonic / timeout: Duration)`); **[173.1 §2 п.5](173.1-parallel-collect-and-supervised-value.md)**
> (`parallel(timeout:)`); **[173 Ф.3-остаток п.5](173-error-system-unify-harden.md)** (удаление `with_timeout` — после этого плана). **Под-план:**
> [175.1 civil-time](175.1-civil-time.md) (D319-D321; добавит 4-й эффект-оп `Time.local_offset()` в схему — §3-нота).
> **Поглощает** `[M-time-now-schema-mismatch]`, `[M-monotonic-mock-support]`, `[M-monotonic-migration-deferred]`;
> финализирует `[M-handler-duration-schema-mismatch]`, `[M-monotonic-per-os-isolated-tests]` (home всех пяти —
> docs/dev/simplifications.md, секция Plan 65; NB: секция там задублирована байт-в-байт ~10372 и ~23886 — дедуп
> отдельным коммитом при закрытии маркеров).
> **Фоновые агенты:** см. §10 (НЕ `git stash`; temp-worktree/commit-reset; идемпотентность под rate-limit).
> **Очередность (граф 173-176 — [README планов §Очередность](README.md), 2026-07-03):** Волна 0 = Ф.0;
> Волна 1 трек C = Ф.1 → Ф.1b → {Ф.1c ∥ Ф.2} → Ф.3 → Ф.4 — **независим от 173/174/176** (стартует сразу).
> **Исходящие:** весь план разблокирует 173 §3a п.3 + 173.1 §2 п.5 (deadline:/timeout:), 173 Ф.3-остаток п.5
> (with_timeout), **176 Ф.2** (Timestamp в Metadata), Plan 66. 175.1 — Волна 3 (после этого плана).
> **Сквозной критерий (обязательный):** «**без упрощений, как для прода**» — формальный критерий приёмки §8.0.

---

## 1. Зачем (вердикт аудита 2026-06-22; file:line актуализированы 2026-07-03)

Time-типы Nova концептуально правильные (`Duration`/`Timestamp`/`Monotonic` — знаковые i64-ns записи; D124 разделяет
wall-clock и monotonic **на уровне типов** — сильнее Go/JS/Java/Zig; `Time` — ambient suspend-эффект D64 →
детерминизм в тестах через handler-подмену, что **строго лучше** Clock-DI у Java/Kotlin/Swift и global-monkey-patch
у JS/tokio, см. §1a). Но есть **7 дефектов поверхности + 1 критический пробел корректности**:

1. **`Time.now() -> int` — нетипизированный провод (`[M-time-now-schema-mismatch]`).** Codegen-схема возвращает
   `nova_int`, а stdlib/handler'ы объявляют `now() -> Timestamp`. `Time.now().minus(other)` роутится по **int-receiver
   path** → ломается method-dispatch. Workaround сейчас — руками `Timestamp.from_unix_millis(Time.now())`.
2. **`Monotonic.now()` — compiler-builtin, а не `.nv`.** Монотонные часы захардкожены в **ЧЕТЫРЁХ** сайтах (Ред. 2
   нашла +2 inference-сайта; **symbol-якорь: grep `nova_monotonic_now_record` + `"Nova_Monotonic*"` по emit_c.rs** —
   строки = снимок @ cc19478b): dispatch [emit_c.rs:25409-25411](../../compiler-codegen/src/codegen/emit_c.rs#L25409)
   (Member) + [:28037-28039](../../compiler-codegen/src/codegen/emit_c.rs#L28037) (Path) → `nova_monotonic_now_record()`
   ([channels.h:1428-1432](../../compiler-codegen/nova_rt/channels.h#L1428)); плюс C-type-inference
   [:39586-39588](../../compiler-codegen/src/codegen/emit_c.rs#L39586) (Member) + [:40272-40274](../../compiler-codegen/src/codegen/emit_c.rs#L40272)
   (Path) → `"Nova_Monotonic*"` — при удалении builtin (Ф.3) убрать ВСЕ четыре. Нарушает §3-правило «брать из `.nv`»
   ([[feedback-maximize-nv-sourcing]]) и делает часы **немокабельными** (`[M-monotonic-mock-support]`).
3. **ПЯТЬ расходящихся источников одной схемы**: (a) prelude-decl [effects.nv:137-140](../../std/prelude/effects.nv#L137)
   (`sleep(ms int)`, `now()->int`); (b) codegen-schema [emit_c.rs:2870-2902](../../compiler-codegen/src/codegen/emit_c.rs#L2870)
   (symbol-якорь `effect_schemas.insert("Time"` :2901; `sleep` :2880 / `now` :2881 / `now_monotonic` :2894 +
   5 timer-счётчиков :2896-2900); (c) C-vtable
   [effects.h:863-869](../../compiler-codegen/nova_rt/effects.h#L863) (ctx/sleep/now/`now_ms`/`now_ns`); (d) handler-литералы
   [handlers.nv:180-221](../../std/testing/handlers.nv#L180) (`now()->Timestamp`, `now_ms`/`now_ns`, `sleep(d Duration)`);
   (e) **закомментированная** decl [duration.nv:541-546](../../std/time/duration.nv#L541). Менять = править 5 мест.
4. **`sleep(ms int)` vs `sleep(d Duration)`.** prelude берёт сырой `int` ms; handler'ы/usage — `Duration`. Bridge
   `[M-handler-duration-schema-mismatch]` частично закрыт (annotation-мост), но канон в decl всё ещё `int`.
5. **Единица неоднозначна** (`now()->int` — ms или ns?). Решено: **ns везде** (§3.0-Q5).
6. **`Time.now()→Monotonic` миграция заморожена** (`[M-monotonic-migration-deferred]`, ≈9 сайтов timing-логики, §6).
7. **`now_ms`/`now_ns`** живут только в vtable+handler'ах (НЕ в codegen-schema) — рудимент int-провода (убрать, §3).
8. **🔴 КРИТИЧНО — Duration-арифметика молча переполняется.** ВСЕ операторы [duration.nv:264-323](../../std/time/duration.nv#L264)
   (`@plus` :264 /`@minus` :269 /`@neg` :274 /`@times(i64)` :279 /`@times(f64)` :287 /`@div(i64)` :292 /`@div(f64)` :297 /
   `@abs` :312) — сырые **unchecked i64**, two's-complement **WRAP** на ±292 годах. Это ровно Go-ловушка («the trap to
   avoid»), а Rust/Java/Kotlin/Temporal/**Swift** детектят overflow (Swift-integers trap by default; Zig — panic только
   в Debug/ReleaseSafe, UB в ReleaseFast — антипример build-mode-зависимости). → Ф.1c + D317: trap в debug И release.

Гражданское время (Date/DateTime/TimeZone/Period, ISO-8601 format/parse) — **вне scope**, под-план
[175.1](175.1-civil-time.md).

## 1a. Где Nova УЖЕ лучше peers — планка 7 языков (зафиксировать в доке как differentiators)

- **Clock-injection через алгебраический эффект-handler — строго лучше 6 из 7; Swift Clock — ближайший, но виральный.**
  Java/Kotlin `Clock`-DI — виральна и **молча падает на real-clock**, если хоть один `now()` забыли пробросить; TS/JS
  (`@sinonjs/fake-timers`; Temporal не решает mock) и tokio `pause` — глобальный monkey-patch (cleanup, concurrency-unsafe); Go 1.25 `synctest` —
  runtime-«пузырь»; **Zig — clock-абстракции НЕТ вообще** (ни injection, ни mock). **Swift Clock (SE-0329)** —
  ближайший прецедент (протокол-абстракция, TestClock, `.measure{}`, typed-deadline `sleep(until:tolerance:)`), но:
  (а) инъекция виральна — generic `<C: Clock>` тащится через все сигнатуры, забыл пробросить → молчаливый real-clock;
  (б) юзер обязан выбирать между ДВУМЯ monotonic-часами на каждом call-site. Nova-handler **лексически скоупнут,
  ambient (не вирусит сигнатуры), композируется, виден в effect-row, работает sync+async, без cleanup**. **НО** заявка
  верна только после Ф.3 (роутинг `monotonic()` через эффект).
- **Compile-time wall-vs-monotonic separation (D124)** бьёт Go (один `Time` со скрытым mode-bit), TS/JS-legacy (голый
  `number`; Temporal.Instant чинит wall, но monotonic-типа в языке нет), Java (`nanoTime` — типонезависимый `long`), **Zig** (wall = голые `i64`/`i128` без типа). Наравне с
  Rust/Kotlin. **Swift здесь сильнее всех** (разделяет на уровне типов даже два monotonic-инстанта:
  `ContinuousClock.Instant` ≠ `SuspendingClock.Instant`) — честно отразить в таблице docs/guide/time.md.
- **Единый `Time`-эффект на wall+monotonic+sleep** решает то, что Kotlin **не может** (ТРИ несвязанных clock-авторитета:
  `Clock`/`TimeSource`/`TestCoroutineScheduler`) и что у Swift разнесено по трём объектам часов. У Nova — один handler
  двигает wall И monotonic И sleep когерентно (тест §7 mock-coherence).
- **`sleep_until(Monotonic)`-only** (нет `sleep_until(Timestamp)`) — дизайн, который Java сделала **неправильно**
  (`parkUntil` на wall → JDK-8146730), Go/JS/Kotlin/Zig вовсе не имеют; **Swift — единственный peer с typed-deadline
  `sleep(until:)`** (паритет; у Swift есть `tolerance` — см. `[M-sleep-tolerance]` §11).
- **`sleep(Duration)` typed** vs **Zig `sleep(u64 ns)`** — голое число = unit-footgun (см. neg-тест §7 `sleep(100)`).
- **Integer-ns канон**: Swift `Foundation.Date` = `Double` секунд — float-потеря точности (антипример, поддерживает Q5).
- **Честные уступки** (задокументировать, не прятать): Zig `nanoTimestamp() -> i128` не имеет 2262-горизонта — Nova
  принимает **i64 ±292y осознанно** (Q11 + Q2 scalar-bridge; Timestamp-окно ≈ 1677-09-21..2262-04-11, Q16); Zig
  `Instant.now()` — error-union honesty про платформы без monotonic — Nova выбирает **infallible-by-contract** на
  tier-1 libuv (Q15).

## 2. Текущая схема (как есть, факты с file:line — актуализировано 2026-07-03)

| Источник | wall | sleep | monotonic | extra | file:line |
|---|---|---|---|---|---|
| prelude decl | `now()->int` | `sleep(ms int)` | — | — | [effects.nv:138-139](../../std/prelude/effects.nv#L138) |
| codegen schema | `now->nova_int` :2881 | `sleep(nova_int)->nova_unit` :2880 | `now_monotonic->nova_int` :2894 | 5×timer-счётчик :2896-2900 | [emit_c.rs:2870-2902](../../compiler-codegen/src/codegen/emit_c.rs#L2870) |
| C-vtable | `now` | `sleep` | — | `now_ms`/`now_ns` | [effects.h:863-869](../../compiler-codegen/nova_rt/effects.h#L863) |
| handlers | `now()=>Timestamp` | `sleep(d Duration)` | — | `now_ms`/`now_ns` | [handlers.nv:180-221](../../std/testing/handlers.nv#L180) |
| commented-out | `now()->Timestamp` | `sleep(d Duration)` | — | `now_ms`/`now_ns` | [duration.nv:541-546](../../std/time/duration.nv#L541) |
| stdlib типы | `Timestamp{nanos i64}` (heap) | — | `Monotonic{nanos i64}` (heap, builtin `now()`) | — | [duration.nv](../../std/time/duration.nv) |

Builtin monotonic: dispatch [emit_c.rs:25409](../../compiler-codegen/src/codegen/emit_c.rs#L25409) (Member) +
[:28037](../../compiler-codegen/src/codegen/emit_c.rs#L28037) (Path) → `nova_monotonic_now_record()` heap-alloc; +2
inference-сайта [:39586](../../compiler-codegen/src/codegen/emit_c.rs#L39586)/[:40272](../../compiler-codegen/src/codegen/emit_c.rs#L40272).
Runtime-часы: `uv_hrtime()` ([fibers.h:2261/2277](../../compiler-codegen/nova_rt/fibers.h#L2261)), `Nova_Time_sleep`
([fibers.h:2905](../../compiler-codegen/nova_rt/fibers.h#L2905); рядом `Nova_Time_now` :2912, `now_ms` :2923, `now_ns` :2930,
`now_monotonic` :2951). 5 observability-счётчиков — **не «время», а timer-runtime-интроспекция** (Plan 65 Ф.11).

## 3. Новая схема (типизированный эффект; один источник)

**Принцип.** `Time` — **внутренний плумбинг-эффект** (как `TcpNet`/`AddrNet`, [net/effect.nv §21-40](../../std/net/effect.nv#L21)):
user-код его **не вызывает напрямую**, а ходит через type-методы. Эффект отдаёт **типизированные value-записи**, не int;
единица — **наносекунды**; схема живёт в **одном** месте (`.nv`-decl), codegen её **читает**; default-handler = тонкие
**`extern "C" fn`**-примитивы — module-private C-символы в `nova_rt` по литеральному имени (как `std/net/ffi.nv`).
**Обоснование (D282):** keyword выбирает только имя эмитируемого символа + проверку C-нативных типов; все 3 хука —
C-нативные скаляры (`int`/`()`), Nova-типизация (`Timestamp`/`Monotonic`/`Duration`) живёт в `.nv`-обёртке (handler),
не в externe → **`extern "C"`** (как net). Impl в C (не выдумывать, [[feedback-maximize-nv-sourcing]] §3).

**Эффект (плумбинг — юзер не трогает; опы названы по возвращаемому типу, как `AddrNet.loopback`/`v4`):**
```nova
type Time effect {
    timestamp() -> Timestamp     // wall-clock read (Unix epoch ns); может прыгать (NTP/DST)
    monotonic() -> Monotonic     // монотонные часы (ns); non-regression + clock-source contract D318
    sleep(d Duration) -> ()      // suspend текущего fiber на >= d (D64, cancellable); d<=0 => немедленно
}
```
*(NB: под-план [175.1](175.1-civil-time.md) добавит 4-й оп `local_offset()` в ЭТУ ЖЕ схему — единый `.nv`-источник
расширяется, не форкается.)*

**TimerMetrics (отдельный read-only surface, Mem-style) — 5 счётчиков ВЫНЕСЕНЫ из `Time`** (решение Q1): они —
интроспекция timer-runtime (Plan 66 territory), не «время», read-only (не suspend). Иначе test-handler'ы вынуждены
стабить 5 бессмысленных опов.

**User-facing surface (на типах + free-fn) — только это видит юзер:**
```nova
Timestamp.now()  => Time.timestamp()        // .nv-сахар
Monotonic.now()  => Time.monotonic()        // .nv-сахар (из compiler-builtin → .nv, Ф.3)
fn sleep(d Duration) Time => Time.sleep(d)  // free, prelude-export (метод-формы d.sleep() НЕТ — neg-тест §7)
fn sleep_until(deadline Monotonic) Time     // монотонный дедлайн (Swift/tokio-паритет; MVP-обёртка, Q3)
```
- `sleep_until` — **только `Monotonic`** (дедлайн иммунен к NTP/DST). Wall-абсолютный сон — **явно** `sleep(ts.time_until())`
  (footgun виден на call-site); `sleep_until(Timestamp)` **не вводим** (`E_SLEEP_UNTIL_WALL` с fix-it на `sleep(ts.time_until())`).
- `sleep` — единственный оп, который юзер зовёт «как есть»; free-обёртка прячет эффект (как net), `Time` виден в сигнатуре.

| Операция | Было | Стало |
|---|---|---|
| wall | `now()->int` | эффект `timestamp()->Timestamp` + сахар `Timestamp.now()` |
| monotonic | builtin (i64), 4 codegen-сайта | эффект `monotonic()->Monotonic` + сахар `Monotonic.now()` (builtin удалён) |
| sleep | `sleep(ms int)` | эффект `sleep(d Duration)` + free `sleep`/`sleep_until` |
| `now_ms`/`now_ns` | vtable+handler-only | **удалить** (= `Timestamp.now().as_unix_millis()`/`…nanos()`) |
| 5 счётчиков | в `Time` | **вынести** в `TimerMetrics` |
| единица | ms/ns дрейф | **ns** канон |

**ABI-ключ.** `Duration`/`Timestamp`/`Monotonic` = `{ ro nanos i64 }`, но **сейчас heap reference-records** (D215:
`{}` = heap). Поэтому Ф.2 предваряется **Ф.1b — миграцией в `value`-records** (прецедент Plan 165): (a) stack/zero-GC;
(b) каждый тип = ровно один i64 → **узкий single-i64 scalar-bridge через границу эффекта provably sound** (Q2),
без блокировки на полный 172.4.

## 3.0. Закрытые решения (бывшие открытые вопросы — РЕШЕНЫ, не «Ф.0 решит»)

| # | Вопрос | РЕШЕНИЕ | Обоснование |
|---|---|---|---|
| Q1 | 5 observability-счётчиков | **Вынести в отдельный `TimerMetrics`-surface** (read-only), убрать из `Time` (Ф.1) | Минимальный плумбинг-эффект; счётчики — Plan 66 territory; не заставлять handler'ы стабить |
| Q2 | record-через-границу | **Узкий single-i64 scalar-bridge поверх Ф.1b**, НЕ блокироваться на 172.4 | Каждый тип = 1×i64 → bridge sound by construction; forward-compatible (172.4 субсумирует) |
| Q3 | `sleep_until` MVP/later | **MVP** (Ф.3): обёртка `sleep(deadline - Monotonic.now())` (saturate-to-zero D318 → прошлый дедлайн = немедленно); true re-arm timer → Plan 66 | ~5 строк, drift-free семантика; typed-deadline имеет только Swift |
| Q4 | `@elapsed_since` vs `@minus(Monotonic)` | **Убрать `@elapsed_since`**, дать overload `@minus(Monotonic)->Duration` + `checked_duration_since(other)->Option[Duration]` | Симметрия с Timestamp; Go-стиль; checked — escape-hatch Rust |
| Q5 | единица | **ns везде** (storage + wire); `now_ms`/`now_ns`-опы убрать | Уже storage-unit; ns = precision-floor uv_hrtime/Rust/Java/Temporal; Swift Date=Double — антипример float |
| Q6 | метод-форма `d.sleep()` | **Нет**, только free `sleep(d)` (+neg-фикстура §7 «d.sleep() → нет метода») | Go/Rust — free fn; «один очевидный способ» |
| Q7 | `Duration.from_days/weeks` | **Оставить** как exact `N×86400s`, задокументировать «не календарный день → `Period` [175.1](175.1-civil-time.md)» | Математически точны; удаление ломает API без выигрыша |
| Q8 | имена эффект-опов | `timestamp()`/`monotonic()`/`sleep()` (по возвращаемому типу) | Симметрия `AddrNet.loopback/v4`; `.now()` — ergonomic-сахар на типе |
| Q9 | **overflow-политика Duration** | **Trap-default операторы + `checked_*`(→Option) + `saturating_*`** (3-tier) | Go-ловушка (silent wrap) недопустима; Swift трапает (прецедент); Zig UB-в-ReleaseFast — антипример; §3b/D317 |
| Q10 | **monotonic регресс** | **Saturate-to-zero** на `@minus`/`elapsed` + `checked_duration_since`→`None`; **без global-lock** (урок Rust 1.60) | HW/VM/OS-баг (JDK-6458294); не паниковать, не лочить hot-path |
| Q11 | **signedness/ширина** | **Signed i64 ns, ±292y, задокументировать границу** (Zig i128 — осознанно НЕ берём: ломает Q2 scalar-bridge и value-ABI ради горизонта >2262) | Уже signed; бьёт Rust unsigned-forces-fallible; цена — Q16 |
| Q12 | **формат `@display`** | **ASCII** (`"us"` не `"μs"`) human auto-scale + отдельная **machine ISO-8601** форма | non-ASCII μs — **U+03BC** (greek mu, байты CE BC) в `@into()` [duration.nv:363](../../std/time/duration.nv#L363) (НЕ U+00B5 и НЕ @into_human — Ред. 2 фактчек) — ломает byte-exact golden-тесты |
| Q13 | Monotonic сериализация | **Запрещена by contract** (process-local); сериализуется только `Timestamp`; Ф.6 **верифицирует** отсутствие derive-пути (если есть — заблокировать + neg) | Go течёт `m=…` в `String()` (footgun); D318 |
| Q14 | **suspend-семантика Monotonic** (Ред. 2, Zig/Swift-аудит) | **Задокументировать per-OS источник** (Linux `CLOCK_MONOTONIC` / macOS `mach_absolute_time` / Win QPC — uv_hrtime), гарантия = ТОЛЬКО монотонность+non-regression, **suspend-inclusion НЕ гарантируется** (unspecified-but-monotonic через сон устройства); ContinuousClock-аналог (BOOTTIME) → `[M-monotonic-boottime]` | Индустрия расходится (Zig=BOOTTIME, Rust/Go=MONOTONIC, Swift=оба) — молчать нельзя; D318 |
| Q15 | **fallibility `monotonic()`** (Ред. 2) | **Infallible BY CONTRACT** (tier-1 libuv: Win/Linux/macOS, uv_hrtime не фейлит); Zig-style error-union отклонён (вирусит call-sites ради платформ, которых нет); порт на платформу без monotonic = отдельное D-решение, НЕ тихий fallback | D316 |
| Q16 | **Timestamp-окно i64** (Ред. 2) | **Задокументировать 1677-09-21..2262-04-11** (unix-epoch ±292y); `from_unix_nanos(i64::MAX)`+`checked_add`→None — pos-фикстура §7; 175.1 не обещает даты вне окна | Цена Q11; D317 |

## 3a. Методы `Duration`/`Timestamp`/`Monotonic`: есть → после рерайта

**Инвариант:** существующий surface сохраняется (рерайт *чинит* int-провод), кроме осознанного `@elapsed_since`→`@minus`.
Меняется представление (Ф.1b heap→value), провод (Ф.2 int→typed); **добавляются** overflow-safe варианты (Ф.1c),
`@display`/`@debug` (D237), `.now()`-сахар, `checked_*`.

**`Duration`** (`#stable 0.1`, [duration.nv:52](../../std/time/duration.nv#L52)):

| Метод | Было | После |
|---|---|---|
| consts ZERO/SECOND/MINUTE/HOUR ([:58-70](../../std/time/duration.nv#L58)); `from_*`/`as_*`/`is_*`/`parts` | работают | без изменений (consts — value-const-evaluable после Ф.1b) |
| `@plus`/`@minus`/`@neg`/`@times(i64\|f64)`/`@div(i64\|f64)`/`@abs` | **unchecked i64 wrap** 🔴 | **trap-on-overflow** (Ф.1c) |
| `checked_add/sub/mul/div(...)->Option[Duration]` | — | **NEW** (Ф.1c) |
| `saturating_add/sub/mul(...)->Duration` | — | **NEW** (Ф.1c, clamp к ±MAX) |
| const `Duration.MAX` (граница saturating; 178 снимает таймаут `@timeout(Duration.MAX)`) | — | **NEW** (Ф.1c; sign-off 2026-07-03 — запрос Plan 178) |
| `try_from_secs_f64`/`@times(f64)`/`@div(f64)` NaN/inf | сырой cast → мусор | **NEW try_*** → `Option`/trap на NaN/inf/overflow (Ф.1c) |
| `@compare` | работает | без изменений |
| `@display`/`@debug` (sink `mut w Write`) | — | **NEW** D237; `@display` = ASCII auto-scale (`"2s"`/`"500ns"`/`"us"`); `@debug` диагностика; машинная ISO-8601 форма отдельно |
| `@into()->str` | **μs U+03BC** в [:363](../../std/time/duration.nv#L363) 🔴 | `@into` делегирует в `@display` (ASCII); `@into_human` ([:389-405](../../std/time/duration.nv#L389), уже ASCII) остаётся extra |

**`Timestamp`** (`#stable 0.1`): + `Timestamp.now() => Time.timestamp()` (**NEW** сахар); `@plus(Duration)`/`@minus(Duration)`
→ **saturate at boundary** + `checked_add/sub->Option[Timestamp]` (**NEW** Ф.1c); `@minus(Timestamp)->Duration` (есть);
`@is_past`/`@time_until`/`@elapsed` — **начинают работать** (Ф.2); `@display`/`@debug` (**NEW**; full-datetime ждёт
[175.1](175.1-civil-time.md), до этого `@debug` = `Timestamp(unix_ns=…)`); сериализуем (только этот тип); **окно
1677..2262 задокументировано** (Q16).

**`Monotonic`** (`#stable 0.6`): `Monotonic.now()` builtin→`.nv` `=> Time.monotonic()` (мокабелен, Ф.3); `@as_nanos`
(есть, escape-hatch); `@plus(Duration)`/`@minus(Duration)`->Monotonic + saturate/checked (Ф.1c — `@plus(Duration)` нужен
и 173 §3a: сахар `timeout:` = `Monotonic.now() + d`); **NEW** `@minus(Monotonic)->Duration` (saturate-to-zero на регресс,
D318) + `checked_duration_since(other)->Option[Duration]` (None на регресс); `@elapsed_since` **УДАЛИТЬ**; `@compare`;
`@display`/`@debug` (`@debug` = offset `Monotonic(+1.234s)`, **не дата**); **non-serializable**; `Monotonic.from_*` —
**НЕ вводить** (opaque, как Rust `Instant`); ⛔ `Monotonic ± Timestamp`/`as_unix_*` → compile-error (D124).

## 3b. Арифметика и overflow (D317 — production-grade; паритет Rust/Java/Swift, бьёт Go/Zig)

- **3-tier дисциплина** (Rust-урок): (1) операторы `+`/`-`/`*`/`/`/унарный `-` → **trap-on-overflow** в debug И release
  (никогда silent wrap — Go-ловушка; и никогда build-mode-зависимость — Zig-антипример UB-в-ReleaseFast; Swift-прецедент:
  integer-арифметика трапает всегда); (2) `checked_*` → `Option[T]` (None на overflow); (3) `saturating_*` → clamp.
- **Граница** = ±(2⁶³−1) ns ≈ **±292 года** (i64); для `Timestamp` = **окно 1677-09-21..2262-04-11** (Q16) — контракт.
- **Асимметрия two's-complement:** `@abs(i64::MIN ns)` НЕ должен быть UB → saturate к `i64::MAX` (Go off-by-1ns) ИЛИ `checked`.
- **`@div(0)`** → trap/`E`; **`@neg(i64::MIN)`** → saturate.
- **Граничная арифметика инстантов:** `Timestamp`/`Monotonic` `@plus(Duration)`/`@minus(Duration)` → **saturate at boundary**
  (зеркало Go `addSec`-clamp) + `checked_*->Option`. `Timestamp - Timestamp` / `Monotonic - Monotonic` → saturating diff.
- **f64-конверсии** (`@times(f64)`/`@div(f64)`/`from_secs_f64`): NaN/inf/overflow → `try_*`→`Option`/trap (Rust паникует);
  не молчаливый мусор-cast.

## 3c. Monotonic: non-regression + clock-source contract (D318)

`monotonic()` читает `uv_hrtime()`. **Контракт из ДВУХ частей (Ред. 2):**
1. **Non-regression:** при кажущемся регрессе (later mark < earlier) **`@minus(Monotonic)` и `elapsed` SATURATE-to-ZERO**
   (никогда negative, никогда panic, **без global-lock** — урок Rust 1.60-saga); `checked_duration_since(other)` → `None`
   на регрессе. Стабильный контракт (не флип-флопить как Rust). `Monotonic` **non-serializable** (process-local).
2. **Clock-source (Q14):** задокументировать per-OS источник uv_hrtime — Linux `CLOCK_MONOTONIC` / macOS
   `mach_absolute_time` (оба suspend-EXCLUDED) / Windows QPC (suspend-поведение платформозависимо). Nova гарантирует
   **только монотонность + non-regression**, НЕ suspend-inclusion; `sleep_until` через сон устройства =
   unspecified-but-monotonic. Индустрия расходится (Zig Instant = CLOCK_BOOTTIME, Rust/Go = MONOTONIC, Swift экспонирует
   ОБА) → молчание = footgun. BOOTTIME-аналог (`ContinuousClock`) — `[M-monotonic-boottime]` (§11), вводить при use-case.

## 4. Фазы (mandatory-now vs later)

**Dep-chain:** Ф.0 → Ф.1 → Ф.1b → {Ф.1c ∥ Ф.2} → Ф.3 → Ф.4 → Ф.5 → Ф.6. (Ф.1c и Ф.2 оба зависят от Ф.1b, но
независимы между собой — параллелятся двумя агентами.) **Коммит после каждой фазы** (§10).

- **Ф.0 — gate (без кода).** Написать черновики **D316/D317/D318** (содержание §3.0/§3b/§3c, вкл. Q14/Q15/Q16) +
  amend-планы D124/D237/prelude-decl. Все Q закрыты (§3.0) — Ф.0 оформляет в D-блоки и проходит spec-review.
  **Ревью = автор** (соло-проект), чеклист: соответствие §3.0-решениям + D-нумерация свободна (D316-318; high-water
  движется — D328 уже занят 172.4) + отсутствие коллизий с 172.x; зарегистрировать `[M-sleep-tolerance]`/
  `[M-monotonic-boottime]` в `docs/plans/backlog-followups.md` (OPEN-view). **GATE:** D-блоки до кода (§5 spec-first).
- **Ф.1 — единый источник схемы (без смены поведения). ✅ ВЫПОЛНЕНО (2026-07-04, ветка `plan-175-time`).** Механизм: codegen **читает** схему `Time` из `.nv`-decl
  (коорд. 172.1 U.1/U.2) вместо хардкода [emit_c.rs:2870+](../../compiler-codegen/src/codegen/emit_c.rs#L2870) (`effect_schemas.insert("Time"`); вынести 5
  счётчиков в `TimerMetrics`; удалить закомментированный 5-й источник [duration.nv:541-546](../../std/time/duration.nv#L541);
  выровнять vtable [effects.h:863](../../compiler-codegen/nova_rt/effects.h#L863). **Содержимое схемы пока НЕ меняем**
  (int-провод остаётся) → поведение не меняется. DEP: Ф.0 (spec-gate); кодовых зависимостей нет (низший риск).
  - **Сделано:** (1) `emit_type_decl`/RUNTIME_DEFINED_TYPES-ветка строит `effect_schemas["Time"]` из `.nv`-decl
    (симметрично RuntimeError/MemOrdering sum-schema); хардкод `effect_schemas.insert("Time")` удалён. (2) `Time`-decl
    в `std/prelude/effects.nv` = единый источник; добавлен `now_monotonic()->int` (был только в хардкоде). (3) **NEW**
    `type TimerMetrics effect { timer_alloc_total/…() -> int }` в effects.nv; C-акцессоры channels.h переименованы
    `Nova_Time_timer_* → Nova_TimerMetrics_timer_*`; `TimerMetrics` добавлен в RUNTIME_DEFINED_TYPES + BUILTIN_VTABLE_NAMES
    + skip в `emit_user_effect_registrations` (direct-C, нет handler-slot'а, как `Mem`). (4) закомментированный 5-й
    источник в duration.nv удалён; vtable-комментарий effects.h выровнен (struct не тронут — now_ms/now_ns ретайр = Ф.2).
    (5) call-сайты `Time.timer_*()` → `TimerMetrics.timer_*()` (nova_tests/plan65 f11/f11a).
  - **Спека:** D316 внесён (04-effects.md) — плумбинг-эффект + единый источник + `TimerMetrics`-split + ns-канон;
    typed-surface/Q15/overflow(D317)/non-regression(D318) — последующие фазы (amend D316). README-нумерация обновлена.
  - **Гейт:** conformance 38/38 (new binary); pos-фикстура `nova_tests/time/plan175_f1_timer_metrics_split.nv` PASS
    (TimerMetrics dispatch + int-провод + Monotonic non-regress); zero-regression **delta = 0** vs parent-бинарь на
    12 dir-сэмпле (temp-worktree `../nova-175-base`, identical pre-existing FAILs); Rust build clean.
  - **NB (pre-existing долг, НЕ Ф.1):** `nova test nova_tests/plan65` целиком не C-компилится из-за
    spawn-closure-captures-module-const дефекта в f10/f7 (bare `TIMEOUT_MS1`/`TIMERS_PER_FIBER` вместо мангл-имени) —
    baseline-delta=0 (тот же CC-FAIL на parent-бинаре); занесён в backlog. Ф.0 (D316-318 полные) + typed-surface — не в scope Ф.1.
- **Ф.1b — value-migration. ✅ SHIPPED (option C, 2026-07-04, ветка `plan-175-time`).** `Duration`/`Timestamp`/`Monotonic`
  `{}`→`value` (single-i64 `nanos`). Конструкторы возвращают ПО ИМЕНИ типа (`-> Duration`/`-> Timestamp`, НЕ `-> Self`)
  — обходит self_value-trap (прецедент VVec4), БЕЗ codegen-зеркала. `DurationParts` остаётся heap. `Monotonic.now()`
  остаётся compiler-builtin, но переведён на **value**-возврат (`(NovaValue_Monotonic){.nanos=_nova_monotonic_ns()}`,
  inline, zero-heap; 6 codegen-сайтов + close_at-проверки обновлены `Nova_Monotonic*`→`NovaValue_Monotonic`).
  **Codegen, потребовавшийся для value-operators (были deferred «Ф.3»):** (a) value-record `@plus/@minus/@times/@div/@rem`
  + `<`/`>`/`<=`/`>=`(@compare) + унарный `@neg` — через by-value wrapper `nova_vr_binop_/unop_` (rvalue-safe, receiver
  ABI `NovaValue_X*`); (b) `infer_expr_c_type` binary-arm возвращает @method return-type (`Timestamp-Timestamp`→Duration);
  (c) `emit_field_eq` scalar-list дополнен raw-C типами (`int64_t` etc) — value-record structural `==` даёт scalar-eq,
  не `memcmp(&rvalue.f,…)`. **C-граница:** value-`Duration` в extern «nova» sync-методы (`wait_for`/`try_*_for`) —
  by-address (materialize temp → `(void*)&tmp`; C читает `*(int64_t*)timeout`); `close_after`/`close_at` — `.nanos`.
  **Handler-capture fix (побочный, был latent-баг):** escaping-фабрики (`fn fixed_ms/mut_clock -> Effect[Time]`)
  капчурили `&stack_local` (dangling после return) → mock-часы читали garbage. Теперь: immutable-scalar → by-value
  snapshot; mutable-scalar в ESCAPING-handler (fn возвращает `NovaVtable_*`) → heap-promote (`nova_alloc` cell); inline
  (fn НЕ возвращает Effect) → by-pointer (сохраняет direct-read-back enclosing-mut, напр. `Counter4.value()=>n`).
  **Гейт:** duration.nv inline-tests PASS; `nova_tests/time/plan175_f1b_value_typed_surface.nv` PASS; handlers.nv PASS
  (fixed_ms+mut_clock); repro `v2_condvar` PASS; conformance-CU компилится + d102 PASS. **Follow-ups (не time-специфичны,
  pre-existing value-record codegen-gaps):** `[M-175-value-record-const-ref]` (value-const-as-value: symbol `ZERO` vs
  ref `Duration_ZERO`), `[M-175-value-in-generic-tuple-return]` (`measure[T]->(T,Duration)` call-site tuple-инференс).
- **Ф.1b — value-migration (исходный enumerated checklist, НЕ «проще»).** `Duration`/`Timestamp`/`Monotonic` `{}`→`value`. По каждому
  риск-сайту аудита — шаг + верификация: (1) 3 типа stack-alloc в 26 методах (ABI); (2) **value-const** ZERO/SECOND/MINUTE/HOUR
  [duration.nv:58-70](../../std/time/duration.nv#L58) + EPOCH [:430](../../std/time/duration.nv#L430) — const-evaluable; (3)
  `DurationParts` (7 полей) **остаётся heap** (display-helper); (4) **D290 generic-forward-decl**: `Option[Duration]`/`Vec[Timestamp]`/
  `Result[Monotonic,E]` — complete struct ДО инстанциирования → Plan 91.12 `/*__VALUE_RECORD_DEFS__*/` **перед
  `/*__MONO_TUPLE_TYPEDEFS__*/`** (Ред. 2-уточнение: эмиссия [emit_c.rs:5333](../../compiler-codegen/src/codegen/emit_c.rs#L5333),
  splice :4660; НЕ «перед NOVAOPT» — тот splice :4675 раньше MONO_TUPLE :4849); (5) монотоник builtin heap-alloc
  (dispatch-сайты п.2 §1) → stack-init; (6) cross-module handler'ы
  [handlers.nv](../../std/testing/handlers.nv); инфра `AllocKind::Value` ([emit_c.rs:2443-2447](../../compiler-codegen/src/codegen/emit_c.rs#L2443))/
  `emit_value_record_type` (:10571)/`NovaValue_` (:10669). DEP: Ф.1. GATE для scalar-bridge.
  - **⚠ Эмпирика 2026-07-04 (заход Ф.2+Ф.1b+Ф.3, ветка `plan-175-time`; код ОТКАЧЕН к parent f4ffe68a — net-zero,
    гейт НЕ пройден): наивный атом (a) `self_value_position_c_type` РЕГРЕССИРУЕТ conformance.** Реализация:
    helper строит по-value C-тип `Self` в return/param-позиции = `receiver_c_type` минус trailing `*` для
    value-структур (`is_value_struct_ptr`), подключён в `resolved_type_to_c("Self")` (:2256) + registration
    return-type (:4017). **Провал:** blanket-стрип `*` для ВСЕХ value-структур ломает установленную семантику
    «value-struct/NamedTuple `-> Self` возвращает POINTER» (Plan 128 mut-receiver fluent-chaining `.push(1).push(2)`
    мутирует ОДИН stack-slot; by-value копия рвёт цепочку). Rust build clean, но `nova test spec_tests/conformance`
    = **37/1** (baseline parent = 38/0): `d102_named_args_default_params` **RUN-FAIL** (baseline PASS, new RUN-FAIL,
    единственный дифф = helper — детерминированно, НЕ флак; сам d102 не содержит `Self`, регресс в prelude/runtime
    value-struct с `-> Self`). **Гоча для следующего захода:** атом (a) обязан быть УЖЕ (gated) — by-value ТОЛЬКО
    для non-mut static value-record конструкторов (`Duration.from_nanos -> Self`), НО СОХРАНЯТЬ pointer-return для
    mut-receiver/fluent-chaining `-> Self`; и верифицировать ПОЛНЫМ conformance, НЕ одной time-фикстурой (single-
    fixture byte-identical ⇏ corpus byte-identical — этот урок стоил ложного «✅ landed»).
  - **🔑 ВАЛИДИРОВАННЫЙ ДИЗАЙН scalar-bridge (эмпирика include-ordering — НЕ затронут откатом, остаётся в силе):**
    generated-C: `#include "nova_rt/nova_rt.h"` (emit_c.rs:5322) идёт ПЕРЕД splice `/*__VALUE_RECORD_DEFS__*/`
    (:5351) → **effects.h НЕ может именовать `NovaValue_Timestamp`** → `NovaVtable_Time` слоты ОБЯЗАНЫ остаться i64.
    Единственно чистый путь = плановый узкий single-i64 bridge в двух сайтах codegen: (i) **call-site** (Member
    :25852 `if effect_schemas.contains_key` + Path :28492) — wrap `((NovaValue_Timestamp){.nanos = Nova_Time_now(...)})`
    для value-struct-ret, unwrap `(<arg>).nanos` для value-struct-арга; (ii) **handler-impl** (`emit_handler_lit`
    :7160) — schema хранит surface-тип (call-site-инференс :39530/:40008), но impl-сигнатура+forward-decl (:7300/:7330)+
    vtable-слот-ассайн (:7280) используют WIRE i64, return-bridge `return (<body>).nanos`, param-bridge для value-struct
    = `NovaValue_Duration d = {.nanos = d_wire};` (существующий annotation-bridge :7458 `(annot)(intptr_t)` —
    pointer-pun, для value-struct НЕ годится). Нужен helper `effect_wire_c_type(schema_c)` (single-i64 value-struct →
    `("nova_int", true, "nanos")`, детект по record-schema, НЕ хардкод §3). effects.h/fibers.h остаются i64
    (rename `now→timestamp` + add `monotonic`-слот + retire `now_ms`/`now_ns`).
  - **Blast-radius реальность (zero-regression гейт):** rename `now()→timestamp()` ломает **277** `Time.now(` сайтов
    в 165 nova_tests-файлах + `now_ms`/`now_ns`-сайты → миграция на `Timestamp.now()`/`.as_unix_*` обязательна В ТОМ ЖЕ
    изменении. Core-codegen (bridge, 2 атома b/c/d, Monotonic-builtin-removal) + runtime + handlers + 277-сайт-миграция +
    тесты = многосессионный объём; НЕ закрыт одной сессией. **Ключевой урок:** Ф.1b/Ф.2 неотделимы (подтверждено §7.7)
    И атом (a) неотделим от полной value-миграции + должен быть gated (иначе conformance-регресс) — весь заход = один
    interlocking commit, промежуточных зелёных нет.
  - **⚠ Эмпирика 2026-07-04 (ТРЕТИЙ заход, ветка `plan-175-time`, код net-zero @ f8cc2d9e — гейт НЕ начат: блокер на
    УРОВНЕ ДИЗАЙНА, до build-циклов). Два новых факта, разведанных этим заходом:**
    1. **Атом (a) self_value_position НЕ НУЖЕН — trap обходится на уровне `.nv`-source.** Установленный рабочий прецедент
       в корпусе: `fn VVec4.new(a,b) -> VVec4` (nova_tests/plan128/t15_value_record_in_named_tuple_ok.nv) — static-конструктор
       value-record'а возвращает по ИМЕНИ типа, НЕ `-> Self`. `-> Self` роутится через `receiver_c_type` (emit_c.rs:12224-12233)
       → `NovaValue_X*` (POINTER, для mut-fluent-chaining Plan 128) → для STATIC-конструктора = указатель на локал = dangling;
       `-> ИмяТипа` роутится через `type_ref_to_c` → `NovaValue_X` by-value (корректно). У Duration/Timestamp/Monotonic
       ВСЕ instance-методы уже возвращают по имени (`-> Duration`/`-> Timestamp`/`-> Monotonic`); `-> Self` только у
       static-конструкторов (`from_nanos`/`from_unix_*`). ⇒ достаточно заменить `-> Self` → `-> Duration` в этих
       конструкторах (+ `value`-keyword в 3 type-decl) — БЕЗ правки codegen, БЕЗ 37/1-регресса. (gate emit_c.rs:4016-4020
       на `is_instance` — доп. страховка, но не требуется.) Проверено чтением codegen; value+explicit-name = уже зелёный паттерн.
    2. **🔴 НАСТОЯЩИЙ блокер (не value, не bridge) — АРХИТЕКТУРНОЕ противоречие prelude ⟷ std.time.** Typed-схема `Time`
       (`timestamp()->Timestamp`/…) требует `Timestamp`/`Duration`/`Monotonic` в scope при schema-build
       (emit_c.rs:10344-10369 зовёт `type_ref_to_c(Timestamp)`, резолв только если тип зарегистрирован value-record'ом
       fwd-decl-препассом emit_c.rs:3031-3041 — т.е. ТОЛЬКО когда `std.time` в CU). НО `Time`-decl живёт в
       `std/prelude/effects.nv`, которая по собственному инварианту = «**ZERO imports, self-contained на primitives**»
       (effects.nv:14) — сослаться на не-примитивные типы там НЕЛЬЗЯ. **Эмпирика корпуса: 85 из 96 файлов, зовущих
       `Time.now`/`Time.sleep`, НЕ импортируют `std.time`** — опираются на prelude-level int-wire `Time`
       (`Time.sleep(5_000)`/`Time.sleep(0)` — bare int ms, 278 вхождений `Time.sleep([0-9]`; `Time.now() - t0` — int-арифметика
       в concurrency-тестах). ⇒ typed-surface НЕ drop-in: требует АРХИТЕКТУРНОГО решения ПЕРЕД миграцией:
       (a) сделать `std.time` prelude-loaded → риск import-cycle (`duration.nv`→`std.testing.handlers`→`duration.nv`) И всё
       равно ломает 85 файлов (bare-int `Time.sleep(N)`); ИЛИ (b) перенести `Time` в `std.time` + добавить `import std.time`
       в ~85 файлов + конвертировать их bare-int sleep/now-арифметику в `Duration`/`Timestamp`. Плановая посылка «единый
       источник схемы в prelude/effects.nv» (Ф.1, D316) для typed-surface **инфизибл** — Ф.1 работала лишь потому, что
       int-wire self-contained на примитивах. Это, вероятно, корень net-zero всех трёх заходов.
    3. **Альтернатива дизайна (для решения владельца): int-wire эффект в prelude + typed-сахар в std.time.** Оставить
       `Time`-эффект int-wire self-contained (`sleep(ns int)`/`timestamp_ns()->int`/`monotonic_ns()->int` — примитивы,
       prelude-ок), а ТИПИЗАЦИЮ вынести чисто в `.nv`-обёртки std.time (`Timestamp.now() => Timestamp.from_unix_nanos(Time.timestamp_ns())`,
       `fn sleep(d Duration) => Time.sleep_ns(d.as_nanos())`). Сносит СРАЗУ и scalar-bridge-codegen (Q2), и prelude-coupling.
       Цена: schema эффекта остаётся int (mock оперирует int ns, не typed-record'ами) — отклонение от Q2/Q8 плана. **Требует
       sign-off: «typed effect surface» (с prelude-архитектурной ценой) vs «typed sugar над int-эффектом» (проще, но mock
       на int).** До этого решения Ф.2 не стартовать — иначе 4-й net-zero.
- **Ф.1c — overflow-safe арифметика (NEW, mandatory). ✅ SHIPPED (2026-07-06, ветка `plan-175-time`).** Реализация — чистый
  `.nv`-слой в `std/time/duration.nv` (codegen НЕ тронут; всё через module-private i64-хелперы). D317/D318 внесены в spec (04-effects.md).
  - **Сделано (D317):** (1) операторы `Duration` `@plus/@minus/@neg/@times(i64|f64)/@div(i64|f64)` — **trap-on-overflow** (debug И
    release) через `*_or_trap`-хелперы поверх явной i64-overflow-детекции (bare `+`/`*` wrap by design → детект явный); (2)
    `@checked_add/sub/mul/div → Option[Duration]`, `@try_mul_f64/@try_div_f64 → Option`, `Duration.try_from_secs_f64 → Option`;
    (3) `@saturating_add/sub/mul → Duration` clamp к **±(2⁶³−1)** (симметрично; `i64::MIN` исключён → `@neg`/`@abs` тотальны);
    (4) `@abs(i64::MIN)` → saturate к MAX; `@div(0)`/`@div(MIN,-1)`/`@neg(MIN)` → trap; `from_secs_f64` теперь trap на NaN/inf/OOR;
    (5) `Timestamp`/`Monotonic` `@plus/@minus(Duration)` + `Timestamp @minus(Timestamp)` → **boundary-saturate** + `@checked_add/sub`;
    2262-окно (Q16). μs U+03BC → уже ASCII `"us"` (снято в Ф.3, подтверждено byte-exact).
  - **Сделано (D318):** `Monotonic @elapsed_since` → **saturate-to-zero** на регресс (никогда negative/panic, без global-lock);
    `@checked_duration_since(other) → Option[Duration]` (None на регрессе, `Some(ZERO)` на равенстве); clock-source/suspend/infallibility
    задокументированы (Q14/Q15). Существующий `@elapsed_since` СОХРАНЁН (rename→`@minus(Monotonic)` — Ф.3c, Ф.2-gated).
  - **Гейт:** cargo build clean (nova-codegen + nova-cli); conformance **PASS** (single-CU, +`d317`/`d318`); inline unit-тесты
    `duration.nv` PASS; trap-фикстуры `nova_tests/time/rt/{dur_add_overflow,dur_div_zero,dur_f64_nan}_traps.nv` PASS
    (`EXPECT_RUNTIME_PANIC`); cross-module `nova_tests/time/plan175_f1c_overflow_safe.nv` PASS; zero-regression **delta = 0**
    (same-binary swap parent↔Ф.1c `duration.nv` на `nova_tests/sync` — результаты byte-identical). Rust build clean.
  - **Отложено (не упрощение — блокер компилятора):** публичные консты `Duration.MAX`/`Duration.MIN` (Plan 178
    `@timeout(Duration.MAX)`) НЕ введены — user type-const `MAX`/`MIN` шэдоуит builtin numeric `.MAX`/`.MIN` в type-set-bound
    generics (spec_tests d310 → CC-FAIL). Saturation-границы = internal `i64_max()`/`i64_min()` (D317 полон). Follow-up
    `[M-175-type-const-max-shadows-builtin]` (checker member-const-резолюция, 172-зона, owner-gated).
  - **Исходное описание Ф.1c:** Реализовать §3b/D317: trap-операторы + `checked_*`/`saturating_*` на Duration;
    boundary-saturate + `checked_*` на Timestamp/Monotonic±Duration; `@abs(i64::MIN)`; `@div(0)`; f64 NaN/inf-policy;
    фикс μs U+03BC в `@into()` (Q12). DEP: Ф.1b. **Здесь Nova достигает паритета Rust/Java/Swift и обходит Go/Zig.**
- **Единицы времени в именах опов — ✅ SHIPPED 2026-07-06 (owner side-task, D316 amend; вне формальной
  Ф-нумерации — НЕ путать с Ф.4 ниже, которая остаётся отдельным TODO).** `Time`-эффект: `now()` → `now_unix_ms()`,
  `now_monotonic()` → `now_monotonic_ns()` (`sleep(ms int)` не тронут — единица уже в имени параметра). Факт-единицы
  подтверждены из runtime: `now_unix_ms` — unix-epoch мс (`Timestamp.from_unix_millis`, `std/time/duration.nv`);
  `now_monotonic_ns` — наносекунды (`_nova_monotonic_ns()` в `nova_rt/fibers.h` оборачивает `uv_hrtime()` без
  деления). Обновлено: schema-decl (`std/prelude/effects.nv`), mock-handlers `fixed_ms`/`mut_clock`
  (`std/testing/handlers.nv`), все вызовы в `std/time/duration.nv`, `std/concurrency/{timer,supervised_deadline_test}.nv`,
  `std/_experimental/concurrency/rate_limiter.nv`. **Найден+починен hardcode-дрейф**, который grep-инвариант из
  рецепта предсказывал: C-side `NovaVtable_Time` (`nova_rt/effects.h`) и wrapper-функции `Nova_Time_now`/
  `Nova_Time_now_monotonic` (`nova_rt/fibers.h`) — hand-written struct/function names, синхронизированы codegen'ом
  designated-init по имени опы (НЕ по хардкод-таблице в `src/`), но сами имена были захардкожены под старые
  `now`/`now_monotonic` → переименованы в `now_unix_ms`/`now_monotonic_ns` синхронно (иначе `NovaVtable_Time` не
  имел бы поля `now_unix_ms` → CC-FAIL на первом же `Timestamp.now()`-вызове; воспроизведено и починено). **Сахар:**
  `Duration.@sleep()` (**NEW**) — `Time.sleep(@to_millis_ceil())`, округляет ВВЕРХ (никогда не спит меньше
  запрошенного). Тест: `std/time/units_test.nv` (5 блоков: Timestamp.now() unchanged, mut_clock-совместимость,
  реальный sleep≥50ms замер по Monotonic, mock-sleep продвигает виртуальные часы, sub-ms округление вверх).
  Spec: [D316 amend](../../spec/decisions/04-effects.md#d316). **Gate:** conformance 54/0; grep-инвариант
  `Time\.now()` в std/+spec_tests = 0; дельта vs main-бинарь (temp-worktree) на nova_tests/time+concurrency —
  0 непредвиденных регрессий (concurrency: `_repro_p110` идентичная pre-existing CC-FAIL до/после; time: 1 ОЖИДАЕМЫЙ
  новый CC-FAIL в `plan175_f1_timer_metrics_split.nv` — старое имя `Time.now()`, nova_tests сознательно НЕ
  мигрирован, уходит в будущую санацию).
- **ИТОГОВЫЙ СТАТУС Ф.2/Ф.3/Ф.5(d) (2026-07-10, sonnet, ветка `time-rework-175`) — читать ПЕРЕД историческими
  блоками ниже (Ф.2/Ф.3/Ф.5 текст ниже остаётся как historical record захода-за-заходом, актуальный итог — здесь):**
  - **Ф.2 (4-й заход) — SUPERSEDED, не 5-й net-zero, а осознанное закрытие.** prelude⟷std.time coupling из 3
    прошлых заходов — РЕШАЕМ (перенос `Time`-decl в `std.time`, схема резолвится). Настоящий барьер ГЛУБЖЕ:
    mock-handler обязан сконструировать `Monotonic` (opaque by contract, без `from_*`) внутри handler-тела, а
    codegen handler-литералов не поддерживает anonymous record-literal (`codegen error: anonymous record
    literal without spread not supported in codegen`). Ни `from_*`-эскейп (подрывает opacity), ни codegen-фикс
    (реальная инженерия, не эта волна) не сделаны — заход ОТКАЧЕН чисто (без diff в дереве). D316-amend (§Ф.2)
    + `docs/guide/time.md` фиксируют находку и рекомендацию: **option C — корректная итоговая архитектура**, а не
    временный обход.
  - **Ф.3(a-d) SHIPPED.** (a) `Monotonic.now()` builtin→`.nv`-сахар — все 4 emit_c.rs-сайта убраны (норматив
    подтверждён: `grep nova_monotonic_now_record` = 0), реальный недостающий кусок был C-vtable-слот
    `now_monotonic_ns` в `NovaVtable_Time` (не архитектура) — закрывает `[M-monotonic-mock-support]`.
    (b) free `sleep(Duration)`/`sleep_until(Monotonic)`. (c) `@minus(Monotonic)` overload (`elapsed_since`
    сохранён). (d) `@display`/`@debug` (D237) на Duration/Timestamp/Monotonic + побочный codegen-фикс
    value-record `${x}`/`${x:?}`-интерполяции (не time-специфичный, полезен для любого будущего value-record).
  - **Ф.4 (sleep-семантика)** задокументирована в `docs/guide/time.md` (d≤0→немедленно, granularity, tolerance-заметка,
    Q7) — decl НЕ typed (Ф.2 superseded), но семантика верна на реальном runtime уровне (`_nova_time_default_sleep`
    уже трактует `ms<=0` как немедленный yield, было ДО этой волны).
  - **Ф.5:** (a)/(b) handlers — closed через Ф.3a (fixed_ms/mut_clock реализуют `now_monotonic_ns`, mock-coherence).
    (c) auto-advance — MVP explicit-advance (через `sleep()`/`Time.sleep()` под `mut_clock`) уже работает и
    покрыт тестами; auto-idle (без явного вызова) — followup, НЕ реализован. (d) `measure[T]` мигрирован на
    `Monotonic` — closes `[M-monotonic-migration-deferred]` **для elapsed-measurement сайтов**; `deadline_in`
    намеренно НЕ мигрирован (return-type committed к `Timestamp`, D124); `is_past`/`time_until`/`@elapsed`
    корректно остаются `Timestamp`-based (старый §6 line-list устарел — эти три НЕ должны мигрировать). (e)
    M:N-контракт задокументирован (`docs/guide/time.md`), не verified тестом под реальной concurrent-нагрузкой.
  - **`within[T]`/`with_timeout[T]` retraction SHIPPED** (Plan 173 §3a п.4, `[M-174-retract-with-timeout]` CLOSED).

- **Ф.2 — типизированный провод (retire int-wire). 🚩 OWNER-GATED design-fork (НЕ реализован; 3 net-zero).** Замена
  int-wire-эффекта на typed-опы (`timestamp()->Timestamp`/`monotonic()->Monotonic`/`sleep(Duration)`) в схеме `Time`
  **архитектурно инфизибл** без разрешения owner'ом связки **prelude ⟷ std.time**: `Time`-decl живёт в
  `std/prelude/effects.nv` (инвариант «ZERO imports, self-contained на примитивах») → не может ссылаться на
  `Timestamp`/`Duration`/`Monotonic`; при schema-build codegen (`type_ref_to_c(Timestamp)`) резолвит их только когда
  `std.time` в CU, а **85 из 96** `Time.*`-файлов НЕ импортируют `std.time` (bare-int `Time.sleep(5_000)`). Опции:
  **(A)** prelude-load `std.time` — риск import-cycle (`duration.nv→std.testing.handlers→duration.nv`) + всё равно
  ломает 85 файлов; **(B)** перенести `Time` в `std.time` + `import std.time` в 85 файлов + конвертировать их bare-int
  sleep/now-арифметику в Duration/Timestamp + scalar-bridge codegen; **(C) SHIPPED (Ф.1b/Ф.3):** int-wire эффект
  (schema без изменений) + typed `.nv`-сахар/методы поверх (`Timestamp.now()=>from_unix_millis(Time.now())`,
  `@is_past = @nanos < Timestamp.now().nanos`), mock оперирует **int ms**. **Рекомендация:** C-as-shipped доставляет
  ПОЛНЫЙ user-facing typed API (value-records + арифметика + is_past/elapsed/time_until + sugar + wait_for(Duration));
  typed-effect-ops (Q2/Q8: mock на typed-record'ах, а не int) — future enhancement ЕСЛИ owner предпочтёт цену
  prelude-разъединения. **До sign-off Ф.2 не стартовать** — иначе 4-й net-zero. Закрытие `[M-time-now-schema-mismatch]`
  — частичное (user-surface typed; wire остаётся int).
- **Ф.2 — типизированный провод — исходный план (superseded by owner-gate выше).** Изменить `.nv`-decl `Time` на typed-surface (`timestamp()->Timestamp`/
  `monotonic()->Monotonic`/**`sleep(d Duration)`** — смена sleep-decl ЗДЕСЬ, Ф.4 остаётся семантика); единый источник (Ф.1)
  пропагирует во все места; узкий scalar-bridge (Q2). Убрать `now()->int`/`now_ms`/`now_ns`. **Закрывает
  `[M-time-now-schema-mismatch]`.** DEP: Ф.1b.
- **Ф.3 — user-facing surface. ✅ SHIPPED (option C, 2026-07-04) — typed `.nv`-слой поверх int-wire (см. Ф.1b-блок).**
  Доставлено: сахар `Timestamp.now()` (обёртка int-wire `Time.now()` ms → value Timestamp, мокабелен через
  fixed_ms/mut_clock); `@is_past`/`@time_until`/`@elapsed` РАБОТАЮТ (int-based: `@nanos` vs `Timestamp.now().nanos`);
  `@plus(Duration)`/`@minus(Duration)`/`@minus(Timestamp)` value; `measure`/`deadline_in` через `Timestamp.now()`;
  μs U+03BC→ASCII `"us"` в `@into()` (Q12). `Monotonic.now()` — value-builtin (эффектонезависим → допустим в
  `realtime{}`; `.nv`-сахар + мокабельность = future, нужен `now_monotonic`-vtable-слот). `@display`/`@debug` (D237),
  `checked_*`/`saturating_*`/trap-overflow (Ф.1c), `sleep_until`/free `sleep`, `@minus(Monotonic)`-overload —
  НЕ в option-C-минимуме (typed-effect-ops-семья / Ф.1c / Ф.4). **Исходный план ниже (typed-effect-based) superseded.**
- **Ф.3 — user-facing surface (исходный typed-effect план; superseded).** (a) сахар `Timestamp.now()=>Time.timestamp()`, `Monotonic.now()=>Time.monotonic()` —
  **удалить ВСЕ ЧЕТЫРЕ builtin-сайта** (норматив — СИМВОЛЬНО: grep `nova_monotonic_now_record` по dispatch-путям
  emit_c.rs = 0; обе inference-ветки `"Nova_Monotonic*"` удалены; снимок строк: dispatch :25409/:28037, inference
  :39586/:40272 — пере-grep перед фазой) (НЕ schema-reg `effect_schemas.insert("Time"` — та уходит в Ф.2),
  runtime-примитив зовётся
  через default-handler → **закрывает `[M-monotonic-mock-support]`**; (b) free `sleep(d Duration) Time` (prelude-export) +
  `sleep_until(deadline Monotonic) Time` (MVP-обёртка); (c) `@elapsed_since` → overload `@minus(Monotonic)->Duration` +
  `checked_duration_since` (D318); **pos-test доказывает `m2 - m1` диспатчится в `@minus(Monotonic)`, не `@minus(Duration)`**;
  если мис-диспатч — фикс резолюции (коорд. 172.1, verifiable fixture); (d) `@display`(ASCII)/`@debug` + machine-форма (D237). DEP: **Ф.1c + Ф.2** (Ф.3c правит duration.nv — не параллелить с Ф.1c).
- **Ф.4 — sleep-семантика + unit-доки** (decl уже typed с Ф.2): `sleep(d<=0)`→немедленно (Go/tokio); задокументировать
  granularity (uv-timer ~1ms) и «sleep гарантирует ≥ d»; **сигнатура future-proof под опциональный `tolerance`**
  (Swift-фича; добавится аддитивно, `[M-sleep-tolerance]`); days/weeks-заметку (Q7); финализировать
  `[M-handler-duration-schema-mismatch]`; handler'ы выровнять. DEP: Ф.3.
- **Ф.5 — handlers + auto-advance + миграция + M:N-контракт.** (a) default-handler: module-private **`extern "C" fn`**
  скаляр-примитивы `time_wall_now_ns() -> int` / `time_monotonic_now_ns() -> int` / `time_sleep_ns(ns int) -> ()`
  (именование `<resource>_<action>` без префикса, D282/net-конвенция; C-реализация поверх существующих
  [fibers.h:2905+](../../compiler-codegen/nova_rt/fibers.h#L2905)); Nova-типизация — в `.nv`-обёртке `real_time() -> Effect[Time]`;
  (b) `fixed`/`mut_clock` под новую typed-схему; (c) **auto-advance virtual clock** (tokio/Kotlin/Go-synctest killer-feature):
  под paused-clock, когда все фибры durably-blocked на `sleep`, handler авто-продвигает время к ближайшему дедлайну
  (hook в park/wake) — если велик, MVP = explicit `advance(d)` + followup auto-idle. **Координация 173 §3a (Ред. 2):
  scope-deadline `supervised(deadline:)` обязан регистрироваться в ТОМ ЖЕ deadline-реестре, что sleep — иначе mock-clock
  не детерминирует `supervised(timeout:)`** (зеркальная нота нужна в 173 §3a п.3 при его исполнении); (d) мигрировать
  timing-сайты (§6) на `Monotonic.now()` → **закрывает `[M-monotonic-migration-deferred]`**; (e) **M:N thread-safety
  контракт**: default-handler stateless/thread-safe; stateful `mut_clock` — virtual-clock-тесты под `NOVA_MAXPROCS=1`
  ([[reference-mn-race-case-study]]); neg-нота. DEP: Ф.4.
- **Ф.6 — тесты + per-OS + spec/docs + Q-sweep.** §7 pos+neg+spec_tests; per-OS monotonicity
  (`[M-monotonic-per-os-isolated-tests]`, опц. dedicated nova_rt/time.c); **верифицировать отсутствие serialize-пути
  Monotonic (Q13) — если есть, заблокировать + neg**; **верифицировать** даёт ли `Time.sleep` в `realtime{}` диагностику
  (реальный код = `E_REALTIME_SYNC_PARK`, [emit_c.rs:25305+](../../compiler-codegen/src/codegen/emit_c.rs#L25305) — снимок) — если
  нет, добавить check; amend D-блоки; **`docs/guide/time.md`** (модель + «было→стало» + таблица «Nova vs
  Go/Rust/TS/Kotlin/Java/**Zig/Swift**» со строками: clock-abstraction / fallibility now() / suspend-семантика monotonic /
  ширина представления (Zig i128 / Swift Int128-atto / Nova i64±292y) / tolerance (Swift-only) / overflow-policy +
  differentiators §1a + паритет Swift `.measure{}` ↔ `Duration.measure[T]`); **Q-sweep в spec/open-questions.ru.md**:
  занести Q3.0 как RESOLVED; отметить частичное закрытие **OQ-Q9 из spec/open-questions.ru.md** («стандартные эффекты не определены» — Time-строка теперь определена D316; НЕ путать с внутренним §3.0-Q9); резолвить
  **Q-with-deadline-vs-within** (ответ: deadline = `Monotonic`-канон, `sleep_until` только Monotonic, wall — явный
  `sleep(ts.time_until())`; scope-форма = `supervised(deadline:)` 173 §3a); обновить пример в **Q-cancel-token-with-timeout**
  (`Time.sleep(5000)` устарел → `sleep(Duration)`). DEP: all.

**DEFERRABLE-LATER (явно НЕ в 175):** true re-arm deadline-timer / `tick_every` → **Plan 66** (разблокируется); полный
multi-field 172.4 (узкий bridge субсумируется); гражданское время → **[175.1](175.1-civil-time.md)**; auto-idle-advance
(если Ф.5 даёт explicit `advance`); `tolerance` у sleep → `[M-sleep-tolerance]`; BOOTTIME-часы → `[M-monotonic-boottime]`.

## 5. Spec / D / Q / docs

- **NEW D316** — «`Time`-эффект: typed plumbing-surface (`timestamp`/`monotonic`/`sleep`) + единый источник (codegen
  читает из `.nv`) + `TimerMetrics`-split + ns-канон + **infallibility-by-contract (Q15)** + non-serializable Monotonic».
- **NEW D317** — «Duration/instant overflow-policy: trap-default + `checked_*`/`saturating_*`; ±292y граница +
  **Timestamp-окно 1677..2262 (Q16)**; `@abs`/`@div(0)`/f64-NaN/inf; boundary-saturate; прецеденты: Swift trap /
  Zig build-mode-UB антипример / Go silent-wrap ловушка».
- **NEW D318** — «Monotonic: non-regression (saturate-to-zero + `checked_duration_since`; без global-lock) +
  **clock-source contract (Q14: per-OS источник, suspend-inclusion не гарантируется)**».
- **amend D124** — оба часовых типа типизированно из эффекта; `Monotonic.now()` = `.nv`-обёртка; `@elapsed_since`→
  `@minus(Monotonic)`; Monotonic non-serializable.
- **amend D237** ([protocols.nv:334/358/405](../../std/prelude/protocols.nv#L334)) — `Duration`/`Timestamp`/`Monotonic`
  реализуют `@display`/`@debug` (sink, ASCII); `@into()` (D73) делегирует в `@display`.
- **amend prelude `Time`-decl** (D11/D14/D62) — плумбинг-эффект, typed-опы, user ходит через типы.
- **error-index (NEW codes):** `E_SLEEP_UNTIL_WALL`; Duration-overflow trap-код; (если вводим) `E_DURATION_F64_NONFINITE`.
  **Realtime:** реальный код = `E_REALTIME_SYNC_PARK` (emit_c.rs :25305+/:27003+ — снимок), НЕ
  `E_EFFECT_REALTIME_VIOLATION` (только в комментах); Ф.6 сначала ВЕРИФИЦИРУЕТ.
- **spec_tests/conformance — ОБЯЗАТЕЛЬНОЕ D-покрытие (методология 2026-06-28; Ред. 2):** NEW
  `d316_time_effect_typed_surface.nv` (typed-опы, единый источник, TimerMetrics-split, ns-канон), `d317_duration_overflow_policy.nv`
  (checked_*/saturating_*, ±292y, 2262-окно; trap-кейсы — rt/-фикстурами §7), `d318_monotonic_non_regression.nv`
  (saturate-to-zero, checked_duration_since→None); amend D124 → существующего d124-файла НЕТ → создать
  `d124_wall_monotonic_separation.nv` (pos) + запреты `Monotonic±Timestamp` → `spec_tests/conformance/neg/`; amend D237 →
  **обновить существующий `d237_protocol_method_naming.nv` В ТОМ ЖЕ изменении** (правило: amend D ⇒ существующие
  d-файлы обновляются вместе) либо peer `d237_time_display_debug.nv`. Все — `module spec_tests.conformance`, локалы с
  префиксом d316_/d317_/d318_ (name-leak); прогон `nova test spec_tests`.
- **docs/** — новый `docs/guide/time.md` (по образцу strings-internals.md); таблица «было→стало»; 7-языковая таблица +
  differentiators §1a; Q-sweep (Ф.6). Убрать упоминания int-провода как «текущего».

## 6. Миграция (§7 compiler-conventions) — blast-radius + точные команды

**Blast-radius (пере-измерен 2026-07-03; вырос ~69% с авторинга):** nova_tests/ — `Time.now(` **277** + `Time.sleep(`
**478** = **755** вхождений в **165** файлах; std/ — 20+19 = **39**; `Monotonic.now(` — 22 (nova_tests) + 7 (std) = **29**;
26 методов duration.nv; codegen-схема [emit_c.rs:2870-2902](../../compiler-codegen/src/codegen/emit_c.rs#L2870).
`uv_hrtime()` в sync_*.h (barrier/condvar/countdown/semaphore) для deadline-mgmt. **Перед Ф.5 — пере-измерить grep'ом**
(числа — снимок), переписать в ТОМ ЖЕ изменении.

**Timing-сайты на `Monotonic.now()` (Ф.5.d, enumerated):** `duration.nv:516` (`is_past`), `:522` (`time_until`),
`:528` (`elapsed`), `:681-685` (`measure[T]` ×2), `:692` (`deadline_in`); тесты: `cancel_cycle_linked_tokens.nv`,
`cancel_during_natural_fire.nv`, `condvar_wait_cancel.nv`, `sleep_real_clock.nv`, `cancel_latency_bench.nv`, rate_limiter.
**Перед Ф.5 — `grep -rn "Time.now()" std/ nova_tests/` для полного списка.**

**Команды верификации (Bash/PS cap = 10мин, [[project-bash-timeout-10min-max]] — дробить; `nova test` требует ЯВНЫЙ путь):**
- single-fixture: `compiler-codegen/target/debug/nova-codegen test-build nova_tests/time/<f>.nv --toolchain clang --keep-artifacts`
- targeted: `nova-cli/target/release/nova.exe test nova_tests/time` (+ `nova test nova_tests/plan65 nova_tests/concurrency` батчами)
- mass compile-errors → **per-file loop** (`nova check FILE` → fix → re-check), НЕ full-regress-в-loop ([[feedback-test-fix-per-file-loop]]).
- codegen-верификация = kill-switch baseline на **том же** бинаре ([[feedback-codegen-dce-verification]]).
- **Пересобрать `nova-cli` после правок `.nv`** (time/sync `.nv` вшиты через `include_str!`) и **mtime-touch `.rs`**
  в worktree перед cargo build (stale-кэш).

## 7. Тесты (pos + neg + rt + spec_tests; раскладка по test-conventions — Ред. 2)

**Раскладка (тема `time`, НЕ per-plan папка):**
- **`nova_tests/time/`** — folder-module `module nova_tests.time`; **позитивы = peer-файлы с test-блоками** (без
  маркера; файлы описательные: `plan175_checked_duration_since.nv`, `d318_monotonic_saturate_zero.nv`, …).
- **`nova_tests/time/rt/`** — standalone `fn main` + `EXPECT_RUNTIME_PANIC` для трапов (Duration-overflow оператор,
  `@div(0)`, f64 NaN→Duration) и `EXPECT_STDOUT` — только где нужен именно stdout.
- **`nova_tests/time/neg/`** — `module neg.<name>`, маркер `// EXPECT_COMPILE_ERROR <substr>` (**БЕЗ двоеточия** —
  двоеточие вошло бы в substring-паттерн; раннер классифицирует по маркеру).
- **unit-тесты арифметики Ф.1c** — inline test-блоки в `std/time/duration.nv` (предпочтительно для unit самого модуля).
- **spec_tests/conformance** — файлы из §5 (d316/d317/d318 + d124 + d237-обновление).
Per-fix verify = targeted fixture; полный прогон — на закрытии фазы (§10-батчи).

**pos:** `Timestamp.now().elapsed()/.minus()/.time_until()` без обёртки (роутинг на Timestamp-методы); `m2 - m1` (overload
`Monotonic @minus(Monotonic)` — assert диспатч, не `@minus(Duration)`); `checked_duration_since` (Some/None); `sleep(Duration)`
+ `sleep_until(Monotonic)` (drift-free цикл) + `sleep(ZERO)`→немедленно; `${d}`/`${d:?}`; **mock через `fixed`/`mut_clock`
— в т.ч. перехват `Monotonic.now()`** (раньше невозможно); **mock-coherence (Swift TestClock-паритет, Ред. 2): ОДИН
`mut_clock`-handler двигает И `timestamp()` И `monotonic()` И sleep когерентно — `advance(d)` сдвигает оба чтения на d**
(у Swift это три разных объекта часов — differentiator под тестом); **auto-advance**: `sleep(10.minutes)` под paused-clock
резолвится мгновенно *(при MVP-ветке Ф.5c — fixture зовёт explicit `advance(d)`; auto-idle-тест уходит в followup)*;
`with Time = …` детерминизм; checked_/saturating_ (Some/None/clamp); **2262-horizon (Q16):**
`Timestamp.from_unix_nanos(i64::MAX)` → `checked_add(Duration.SECOND)` = None, `@plus` = saturate (не wrap в 1677);
value-const ZERO/SECOND/MINUTE/HOUR/EPOCH компилируются как const после Ф.1b.

**neg (`EXPECT_COMPILE_ERROR`):** `Monotonic ± Timestamp` / `Monotonic.as_unix_*`/`from_unix_*` → нет метода (D124);
`sleep_until(Timestamp)` → `E_SLEEP_UNTIL_WALL` (+ fix-it); `Monotonic.from_*` → нет; **`d.sleep()` → нет метода (Q6)**;
**`sleep(100)` голым int → нет неявного int→Duration (анти-Zig-footgun, Ред. 2)**; (после верификации §5) `Time.sleep`
внутри `realtime{}` → `E_REALTIME_SYNC_PARK`.

**rt (`EXPECT_RUNTIME_PANIC`):** Duration-overflow оператор → trap; `@div(0)`; f64 NaN→Duration.

**byte-exact / контрактные:** **`@display` без байтов > 0x7F** (ASCII `"us"`, не U+03BC) — отдельный assert; Duration `"0s"`;
trailing-fractional-zeros обрезаются; **Monotonic `@debug` = offset, не дата**; **Monotonic не сериализуется** (Ф.6
верифицирует отсутствие derive-пути; если есть — блок + neg).

**per-OS:** monotonicity (`monotonic()` не убывает) Win+Linux; wall vs monotonic не путаются; под `NOVA_MAXPROCS=1`/AUTOARM
для timing-фикстур ([[reference-mn-race-case-study]]).

## 8. Критерии приёмки

0. **🔴 ОБЯЗАТЕЛЬНО: «без упрощений, как для прода».** Ни одной молча-переполняющейся операции, ни одного «решим потом»
   на критическом пути, ни одного untested поведения; каждая behavior-change закрыта pos+neg-фикстурой + аргументом звучности.
1. `Time`-эффект — typed плумбинг: `timestamp()->Timestamp`/`monotonic()->Monotonic`/`sleep(Duration)`; int-провод
   (`now()->int`/`now_ms`/`now_ns`/`now_monotonic`) ретайрнут; 5 счётчиков в `TimerMetrics`.
2. Схема `Time` — **один** источник (codegen читает из `.nv`); 5-й закомментированный источник удалён.
3. User-facing: `Timestamp.now()`/`Monotonic.now()` (сахар; **все 4 builtin-сайта удалены — норматив символьный:
   grep `nova_monotonic_now_record` в dispatch-путях = 0, обе inference-ветки `"Nova_Monotonic*"` удалены**, мокабельны) + free `sleep` + `sleep_until(Monotonic)`; `@display`(ASCII)/`@debug` на трёх
   типах; `@elapsed_since` убран → `@minus(Monotonic)` + `checked_duration_since`.
4. **🔴 Overflow-safe (D317):** ни один Duration-оператор не wrap'ает молча (trap в debug И release); есть
   `checked_*`/`saturating_*`; Timestamp/Monotonic±Duration saturate at boundary; `@abs(i64::MIN)`/`@div(0)`/f64-NaN/inf
   обработаны; ±292y и **Timestamp-окно 1677..2262** задокументированы (+2262-horizon pos-фикстура).
4a. **sleep-контракт задокументирован (Ф.4):** `d<=0` → немедленно; гарантия «≥ d»; granularity (~1ms uv-timer);
   сигнатура future-proof под `tolerance`; days/weeks-нота Q7 — всё в D316/docs/guide/time.md.
5. **Monotonic (D318):** `@minus`/`elapsed` saturate-to-zero; `checked_duration_since`→None на регрессе; без global-lock;
   non-serializable (верифицировано Ф.6); **per-OS clock-source задокументирован, suspend-семантика оговорена (Q14)**.
6. **`@display` byte-exact ASCII** (нет байтов >0x7F; U+03BC из `@into()` устранён); machine-форма round-trip'ит.
7. `Duration`/`Timestamp`/`Monotonic` — `value`-records (стек); value-const'ы компилируются; `Monotonic` без `from_*`.
8. Timing-сайты мигрированы на `Monotonic`; mock перехватывает `Monotonic.now()`; **mock-coherence тест зелёный**;
   auto-advance (или explicit `advance` + followup-маркер).
9. Закрыты `[M-time-now-schema-mismatch]`/`[M-monotonic-mock-support]`/`[M-monotonic-migration-deferred]`; финализированы
   `[M-handler-duration-schema-mismatch]`/`[M-monotonic-per-os-isolated-tests]` (home = simplifications, секция Plan 65 —
   обновить ОБЕ копии задубленной секции или сначала дедуп).
10. **Гейт корректности (Ред. 2):** spec_tests/conformance зелёный (d316/d317/d318 + d124 + d237) + **nova_tests
    baseline-delta = 0** (baseline = parent-коммит через temp-worktree/commit+reset — §10; nova_tests НЕ гейт корректности
    сам по себе, [[feedback-nova-tests-not-correctness-gate]]); 755+ call-сайтов компилируются = тот же baseline-delta
    прогон (батчами <10мин); realtime{}-бан сохранён (верифицированным кодом).
11. spec: D316/317/318 написаны + amend D124/D237/prelude-decl + **spec_tests-файлы §5**; `docs/guide/time.md` с 7-языковой
    таблицей; differentiators §1a; Q-sweep выполнен (Q9-строка/Q-with-deadline-vs-within/Q-cancel-token-with-timeout);
    followup-маркеры `[M-sleep-tolerance]`/`[M-monotonic-boottime]` зарегистрированы в OPEN-view беклога.

## 9. Конвенции + координация

§1 (проверки в чекере), §3 (схема/типы из `.nv`), §5 spec-first, §6 (коды ошибок + error-index), §7 (blast-radius +
чистый бинарь), §8 (pos+neg, C-codegen). **Координировать с 172.1** (схема из `.nv` + overload-резолюция
`@minus(Monotonic)`) и **172.4** (узкий bridge субсумируется). **173-семья ЖДЁТ этот план** (шапка: supervised/parallel
deadline-параметры; mock-clock deadline-registry — Ф.5c-нота). `02-types.md`/effect-схему — не править в одиночку
(зона 172). **Разблокирует Plan 66.** После каждой большой задачи — project-creation.txt + discussion-log (nova-private)
+ simplifications.md ([[feedback-update-logs]]); источник истины — README планов/simplifications/nova-private
([[feedback-no-external-memory-for-project-state]]).

## 10. Фоновые агенты (если используются при выполнении)

- **НЕ `git stash`** — worktree делят один `.git` → stash/refs repo-global ([[feedback-worktree-shared-stash]]).
  Baseline — **temp-worktree** (`git worktree add ../nova-175-base <parent>`) **или** commit+reset, **никогда** stash.
  Постоянный worktree **`nova-p175`** (naming nova-pNN) создать **первой** Bash-командой и самозарегистрироваться;
  cwd дрейфует → **префикс абсолютным путём в каждой команде** ([[feedback_worktree_cwd_clarity]]).
- **Git:** add только конкретные файлы (никогда `-A`/`.`); `git diff --cached --stat` перед commit; **DCO `git commit -s`**
  (CI-гейт); без `Co-Authored-By`; коммит после каждой фазы, маленькими, без amend; **sync в main после фазы
  bidirectional** (pull main → ветка, merge ветка → main).
- **Идемпотентность под rate-limit** (workflow-агенты ловят серверный rate-limit и падают mid-run): шаги идемпотентны +
  checkpoint (commit per task); скрипты `.filter(Boolean)`, `resumeFromRunId` для резюма; не зависеть от успеха каждого агента.
- **Тесты:** только C-codegen (`nova test`/`test-build`), не интерпретатор ([[feedback-no-interpreter]]); **`nova test`
  требует ЯВНЫЙ путь**. **Батч-рецепт полного прогона:** циклом `nova test nova_tests/<dir1> nova_tests/<dir2> …
  --results-file rN.json` (каждый батч <10мин), хвост `--rerun-failed`; ОТДЕЛЬНО `nova test spec_tests` и `nova test std`;
  флака ≠ регрессия (сверять на ТОМ ЖЕ бинаре). **Гейт корректности = spec_tests + pos+neg фикстуры + baseline-delta**;
  nova_tests сам по себе — только delta ([[feedback-nova-tests-not-correctness-gate]]).
- **Worktree setup** ([[project-worktree-nova-test-setup]]): env `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → main repo;
  libuv-submodule скопировать + удалить `.git`; **mtime-touch `.rs`** перед cargo build; **пересобрать `nova-cli`
  после правок `.nv`** (`include_str!`).
- **Не выдумывать синтаксис** — `spec/decisions/` + `examples/` ([[feedback_nova_syntax]]).

## 11. Followup

`[M-175-time-system-rework]`. Поглощает `[M-time-now-schema-mismatch]` (Ф.2), `[M-monotonic-mock-support]` (Ф.3),
`[M-monotonic-migration-deferred]` (Ф.5) — home всех: simplifications, секция Plan 65 (задублена — дедуп отдельным
коммитом). Гражданское время → [175.1](175.1-civil-time.md) (перенумерация-sweep под-плана выполнен Ред. 2).
`tick_every` + re-arm deadline-timer → **Plan 66**. **NEW (Ред. 2):** `[M-sleep-tolerance]` (Swift-style tolerance у
sleep — энергоэффективность/coalescing; сигнатура future-proof с Ф.4); `[M-monotonic-boottime]` (ContinuousClock-аналог,
CLOCK_BOOTTIME/QueryInterruptTime — при use-case «дедлайны через сон устройства»); auto-idle-advance (если Ф.5 даёт
explicit `advance`). Оба NEW-маркера зарегистрировать в `docs/plans/backlog-followups.md` (OPEN-view) при Ф.0.
