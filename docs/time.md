# Система времени в Nova — `Time`-эффект, `Duration`/`Timestamp`/`Monotonic`

> Plan 175 (time-system-rework). Гражданское (календарное) время — отдельный
> документ [`datetime.md`](datetime.md) (Plan 175.1, `std/time/civil`).

## Модель

`Time` — **внутренний плумбинг-эффект** (как `TcpNet`/`AddrNet`, `std/net/effect.nv`):
пользовательский код НЕ вызывает его напрямую, а ходит через типы и свободные функции:

```nova
import std.time.duration

with Time = th.mut_clock(0 as u64) {   // подмена часов в тестах (D11/D61)
    ro start = Monotonic.now()
    sleep(500.millis())
    ro elapsed = Monotonic.now().elapsed_since(start)
    assert(elapsed == 500.millis())
}
```

Три типа, три роли (не смешиваются — D124 разделяет их на уровне типов):

| Тип | Роль | Источник | Идёт назад? | Сериализуется? |
|---|---|---|---|---|
| `Timestamp` | wall-clock, Unix epoch ns | `gettimeofday`/`GetSystemTimeAsFileTime` | да (NTP/DST) | да |
| `Monotonic` | процесс-локальный монотонный момент | `CLOCK_MONOTONIC`/QPC (`uv_hrtime`) | никогда (saturate-to-zero при кажущемся регрессе) | **нет** (opaque, process-local) |
| `Duration` | длительность, знаковая, ±292 года | — (чистая арифметика) | — | да |

Схема эффекта (внутренняя, `std/prelude/effects.nv` — единый источник, codegen читает
из `.nv`, не хардкодит):

```nova
export type Time effect {
    sleep(ms int) -> ()
    now_unix_ms() -> int
    now_monotonic_ns() -> int
    local_offset_sec() -> int
}
```

`local_offset_sec()` (Plan 175.1, D316 amend + D321, 2026-07-10) — системный
UTC-сдвиг ТЕКУЩЕЙ локальной зоны машины, в секундах (owner decision: системная
зона ДОЛЖНА быть доступна). Nova-сахар: `Offset.local()`
(`std/time/civil/offset.nv`) — closes `[M-175.1-local-offset-effect-op]`.
Только числовой сдвиг — зона в `ZonedDateTime` остаётся ЯВНОЙ (D319 R1),
никакого implicit-fallback на «локальную зону».

**Wire остаётся int** (см. «Ф.2 — почему typed-wire не отгружен» ниже) — весь
user-facing surface, тем не менее, **полностью typed** и **полностью мокабелен**,
включая `Monotonic` (Plan 175 Ф.3a).

## Было → стало

| Операция | Было (до Plan 175) | Стало |
|---|---|---|
| wall-clock read | `Time.now() -> int` (schema/runtime mismatch — `[M-time-now-schema-mismatch]`) | `Timestamp.now()` (typed сахар над int-wire `Time.now_unix_ms()`) |
| monotonic read | compiler-builtin, 4 хардкод-сайта в `emit_c.rs`, немокабелен | `Monotonic.now()` — обычная `.nv`-функция, мокабельна через `with Time = handler {...}` |
| sleep | `Time.sleep(ms int)` голый ms | эффект (int-wire) + free `sleep(d Duration)`/`sleep_until(deadline Monotonic)` |
| `now_ms`/`now_ns` | vtable+handler-only рудимент | не существуют (были только int-wire артефактом) |
| 5 timer-счётчиков | внутри `Time` | вынесены в отдельный read-only `TimerMetrics` |
| единица | ms/ns дрейф между источниками | ns канон (storage); имена опов несут единицу (`now_unix_ms`/`now_monotonic_ns`) |
| overflow | молчаливый two's-complement wrap на ±292 годах | trap-on-overflow (debug И release) + `checked_*`/`saturating_*` (D317) |
| `m2 - m1` | не было typed API | `@minus(Monotonic)`/`elapsed_since` — saturate-to-zero (D318) |
| `@display` | `"μs"` (U+03BC, не ASCII) в `@into()` | ASCII `"us"`; byte-exact `@display`/`@debug` (D237) на всех трёх типах |
| elapsed-measurement | `measure[T]` мерил через `Timestamp.now()` (wall-clock — NTP/DST skew уязвимость) | `Monotonic.now()` (иммунно к wall-clock skew) |

## Overflow-политика (D317) — 3-tier дисциплина

1. **Операторы траппят.** `+`/`-`/унарный `-`/`*`/`/` на `Duration` — panic на overflow,
   **в debug И release** (никогда silent wrap — Go-ловушка; никогда build-mode-
   зависимость — Zig `ReleaseFast`-UB антипример).
2. **`checked_*` → `Option[T]`.** `checked_add`/`checked_sub`/`checked_mul`/`checked_div`
   (Duration); `checked_add`/`checked_sub` (Timestamp); `checked_duration_since` (Monotonic).
3. **`saturating_*` → clamp** к ±(2⁶³−1) ns (≈±292 года).

`Timestamp` дополнительно ограничена **окном 1677-09-21 .. 2262-04-11** (i64 ns вокруг
Unix epoch) — `checked_add`/`checked_sub` возвращают `None` за пределами, голый
`@plus`/`@minus` — saturate (никогда wrap обратно в 1677).

```nova
ro d = Duration.from_nanos(i64_max())
d.checked_add(1.nanos())     // → None (не паника, explicit escape hatch)
d.saturating_add(1.seconds()) // → clamp к i64_max()
d + 1.nanos()                 // → trap (оператор — default-safe)
```

## Monotonic: non-regression + clock-source (D318)

- **Non-regression:** `@minus(Monotonic)`/`elapsed_since` **saturate-to-zero** при
  кажущемся регрессе (HW/VM/OS-баг, ср. JDK-6458294) — никогда negative, никогда
  panic, **без global-lock** (урок Rust 1.60-saga — Rust один раз запаниковал на
  таком регрессе, потом откатил на saturate; Nova фиксирует saturate сразу и
  навсегда, не флип-флопит).
- **Clock-source (per-OS):** Linux `CLOCK_MONOTONIC` / macOS `mach_absolute_time` /
  Windows QueryPerformanceCounter (через `uv_hrtime()`). Гарантия — **только**
  монотонность + non-regression; **suspend-inclusion НЕ гарантируется** (сон
  устройства — unspecified-but-monotonic). `ContinuousClock`-аналог (BOOTTIME)
  не введён — `[M-monotonic-boottime]`, при появлении use-case.
- **Opaque by contract** — нет `Monotonic.from_*` (как Rust `Instant`): единственный
  способ получить `Monotonic` — `Monotonic.now()` или арифметика над существующим
  значением. Это защищает от фабрикации фейковых монотонных моментов и — важное
  архитектурное следствие — является причиной, почему typed-эффект-wire (см. ниже)
  архитектурно дороже, чем кажется на первый взгляд.
- **Non-serializable** — `Monotonic` НЕ имеет `#impl(Serialize)` (verified,
  `spec_tests/conformance/neg/d316_monotonic_non_serializable_neg.nv`): process-local
  значение, бессмысленное вне процесса (антипаттерн Go, где `Time.String()` может
  утечь `m=…` monotonic-компонент в лог).

## Sleep-семантика (Ф.4)

- `sleep(d)`/`Duration.@sleep()` — `d <= 0` резолвится **немедленно** (Go/tokio
  parity), никогда не паникует на нулевой/отрицательной длительности.
- Гарантия — **«спит НЕ МЕНЬШЕ `d`»**, granularity — libuv timer wheel (~1ms).
- `sleep_until(deadline Monotonic)` — MVP-обёртка `sleep(deadline.elapsed_since(now))`;
  дедлайн уже в прошлом → saturate-to-zero → немедленно. Drift-free true re-arm
  timer — future work (Plan 66).
- Сигнатура future-proof под опциональный `tolerance` (Swift `sleep(until:tolerance:)`
  паритет — энергоэффективность/coalescing) — `[M-sleep-tolerance]`, не введён.
- `sleep_until` принимает **только** `Monotonic` — `sleep_until(Timestamp)` не
  вводится (wall-clock дедлайн иммунен к NTP только через monotonic; явная
  wall-alternative — `sleep(ts.time_until())`, footgun виден на call-site).

## Мокабельность (AI-first testing)

Один handler двигает **и** wall, **и** monotonic **и** sleep когерентно (Ред.2 Q14 —
Swift `TestClock`-паритет, но БЕЗ вирального generic-параметра на каждой сигнатуре):

```nova
import std.testing.handlers as th

test "rate limiter refills after 1s" {
    with Time = th.mut_clock(0 as u64) {
        ro m0 = Monotonic.now()
        sleep(1.second())               // виртуальные часы, не реальное ожидание
        assert(Monotonic.now().elapsed_since(m0) == 1.second())
        assert(Timestamp.now().as_unix_millis() == 1000)  // ОДИН источник, оба сдвинулись
    }
}
```

`fixed_ms(ms)` — часы замерли (детерминированные timestamps, `sleep` — no-op).
`mut_clock(start_ms)` — виртуальные часы, `sleep`/`Time.sleep` продвигают их без
реального ожидания.

**Auto-idle-advance (Plan 175, owner TODO closure, 2026-07-10):** tokio
`time::pause()` / Kotlin `TestCoroutineScheduler.advanceUntilIdle()`-паритет —
конкурентные `spawn`-фибры под ОДНИМ `mut_clock` больше не требуют явного
`sleep()`-вызова на каждый шаг. Каждый `sleep(ms)` вычисляет свой АБСОЛЮТНЫЙ
дедлайн (`current_ms + ms`, до парковки) и паркует вызывающий фибр
(`vclock.park_until`, `nova_vclock_park_until` в nova_rt/fibers.h) в per-scope
registry; когда ВСЕ живые фибры scope'а виртуально запаркованы (idle — реальной
работы не осталось), просыпается ближайший по дедлайну (может быть другой
фибр) — часы продвигаются `current_ms = max(current_ms, deadline)` (не `+=`,
чтобы не задвоить вклад уже сработавших siblings). Плоский sequential-поток
(нет фибра вообще) резолвится немедленно — ПОВЕДЕНИЕ НЕ ИЗМЕНИЛОСЬ для
overwhelmingly common случая. Тесты — `std/testing/handlers.nv` (tokio-style
`sleep(10_000)` мгновенно; три конкурентных `sleep` с разной длиной будятся
в порядке дедлайна, не spawn-порядка; финальные часы = max, не сумма).

```nova
test "конкурентные sleep будятся в порядке дедлайна" {
    with Time = th.mut_clock(0 as u64) {
        supervised {
            spawn { Time.sleep(100); /* ... */ }   // проснётся ТРЕТЬИМ
            spawn { Time.sleep(10);  /* ... */ }   // проснётся ПЕРВЫМ
            spawn { Time.sleep(50);  /* ... */ }   // проснётся ВТОРЫМ
        }
    }
}
```

**M:N-контракт:** default (real-clock) handler stateless/thread-safe. `mut_clock`
**stateful** (мутирует захваченную `current_ms`; auto-idle-advance добавляет
non-atomic per-scope registry поверх) — под concurrent `spawn`/`parallel for`
нужен `NOVA_MAXPROCS=1` (детерминизм гонки записи в handler-state — см.
[[reference-mn-race-case-study]]).

**`[M-175-vclock-armed-mn-scope-identity]` (задокументированное сужение):**
deadline-order гарантия auto-idle-advance проверена и держит под кооперативным
spawn-путём (`NOVA_MAXPROCS=1` + `NOVA_AUTOARM=0` — cooperative/local
`nova_fiber_spawn_into`, где `_nova_active_scope` внутри фибра — ОБЩИЙ scope
всего `supervised{}`-блока). Под ДЕФОЛТНЫМ armed M:N runtime (auto-arm на
первом `spawn`) `_nova_active_scope` внутри фибра — это WORKER'а СОБСТВЕННЫЙ
`w->scope` (`_worker_run_one_fiber`), не общий scope siblings — registry не
шарится корректно между siblings, механизм деградирует БЕЗОПАСНО (каждый
virtual sleep всё равно резолвится, без hang/crash), но БЕЗ гарантии порядка
по дедлайну (вместо этого — spawn-порядок, старое поведение). Починка общего
M:N-случая требует другого якоря (например резолв через
`NovaSpawnCtxBase._nova_parent_scope`) — вне периметра этого захода.

## Ф.2 — почему typed effect-wire не отгружен (архитектурная находка)

Исходный план предполагал ретаксацию int-wire на полностью typed схему
(`timestamp() -> Timestamp`/`monotonic() -> Monotonic`/`sleep(d Duration)` — прямо
в декларации эффекта). Четыре захода (включая этот) показали: prelude⟷std.time
coupling решаем (перенос декларации `Time` в `std.time`, рядом с типами), но
упирается в **более глубокий** барьер — mock-handler обязан **сконструировать**
typed `Monotonic`-значение внутри тела handler'а, а (a) `Monotonic` намеренно
opaque (нет публичного конструктора) и (b) codegen handler-литералов не
поддерживает anonymous record-literal. Экспонировать internal-конструктор
специально для test-handler'ов подрывает opacity-контракт (тот же конструктор
виден и обычному юзер-коду).

**Вывод:** отгруженная архитектура — typed `.nv`-сахар ПОВЕРХ int-wire эффекта
(`Timestamp.now()`/`Monotonic.now()`/free `sleep`/`sleep_until`) — не временный
compromise, а корректное итоговое решение при текущих возможностях компилятора:
typed-обёртка живёт в родном модуле типа (где anonymous record-literal —
обычный function body, не handler-литерал), поэтому opacity и codegen-ограничение
не конфликтуют. `[M-time-now-schema-mismatch]` закрыт **частично по конструкции**
(user-surface полностью typed и мокабелен; wire — int).

**UPD 2026-07-10 (волна handler-annot):** codegen-ограничение (b) — anonymous
record-literal в handler-теле — **снято** (единый канал типовой разметки подведён
к эмиссии оп-тел; см. D316-amend UPD в `spec/decisions/04-effects.md` и матрицу
`nova_tests/plan175_handler_annot/repro_matrix.nv`). На архитектуру `Time` это
НЕ влияет: барьер (a) — намеренная opacity `Monotonic` — самодостаточен, option C
(int-wire + typed-сахар) остаётся итоговым решением владельца; провод `Time`
не менялся.

## Nova vs 7 языков

| | Go | Rust | TypeScript/JS | Kotlin | Java | Zig | Swift | **Nova** |
|---|---|---|---|---|---|---|---|---|
| wall vs monotonic — раздельные типы | нет (один `Time`, mode-bit) | да (`SystemTime`/`Instant`) | нет (`Date`/`performance.now()` — оба голые числа) | нет (`Clock`/`TimeSource`/`TestCoroutineScheduler` — ТРИ несвязанных) | частично (`Instant`/`nanoTime()` — `long`, не тип) | нет (голые `i64`/`i128`) | да (сильнее всех — ДВА разных monotonic: `ContinuousClock`/`SuspendingClock`) | да (D124) |
| clock injection / mock | monkey-patch/`synctest`-bubble | нет std (crates) | `@sinonjs/fake-timers` (monkey-patch) | `Clock`-DI виральна, молча падает на real-clock если забыли пробросить | DI виральна | **нет вообще** | `Clock`-протокол, `TestClock`, но виральна (`<C: Clock>` через все сигнатуры) | **handler лексически скоупнут, ambient, не вирусит сигнатуры** (D11/D61) |
| `now()` fallibility | infallible | infallible | infallible | infallible | infallible | **error-union** (честно про платформы без monotonic) | infallible | infallible-by-contract (tier-1 libuv; Q15) |
| overflow policy | **silent wrap** (антипаттерн) | trap (panic) | float precision loss | JVM `long` wrap | JVM `long` wrap | **UB в ReleaseFast** (build-mode-зависимо) | trap (integer-арифметика трапает всегда) | trap (debug И release) + `checked_*`/`saturating_*` |
| instant width | `int64` ns (монотонный компонент) | `i64`+`u32` (сек+наносек) | `f64` ms (float!) | `Long` ns | `long` ns | **`i128`** (нет 2262-горизонта) | `Int128`-подобная (atto-эпоха, широкая) | `i64` ns, **±292y, документированная граница** (Q11/Q16) |
| `sleep`/`sleep_until` typed | голый `time.Duration`(int64) | typed (`Duration`) | голый ms (`number`) | typed | typed | голый `u64 ns` (footgun) | typed, **+`tolerance`** (уникально) | typed, `tolerance` — future (`[M-sleep-tolerance]`) |
| `sleep_until` clock | wall (`time.Time`) | оба (`Instant`/`SystemTime`) | нет прямого аналога | оба | wall (`parkUntil`, JDK-8146730 — баг!) | нет | оба (`Clock.sleep(until:)`) | **только Monotonic** (запрещает wall-based sleep_until типобезопасно) |

## Footguns, задокументированные явно

- **`sleep(100)` — компиляция ошибка** (нет implicit int→Duration): анти-Zig-footgun
  (`sleep_bare_int_neg`).
- **`sleep_until(Timestamp)` — компиляция ошибка** (E7301 type-mismatch): дедлайн
  через wall-clock иммунен к NTP только явно (`sleep(ts.time_until())`).
- **`Monotonic ± Timestamp` — компиляция ошибка** (нет overload): смешивание
  доменов часов невыразимо на уровне типов (D124).
- **`Monotonic.as_unix_*`/`.from_*` — нет метода**: opaque-контракт.
- **`d.sleep()` (метод-форма) — ДОСТУПНА** (owner side-task, `Duration.@sleep()`,
  2026-07-06) — свободная `sleep(d)` остаётся каноном (Q6/Q8: юзер не трогает
  `Time` напрямую), но метод-форма НЕ запрещена (это отличается от исходного
  §3.0-Q6 замысла плана — амендмент фактом).
- **`Time.sleep` внутри `#realtime fn`** — компиляция ошибка (D64 suspend-effect
  ban), но диагностика — plain-message, НЕ именованный `E_REALTIME_SYNC_PARK`
  (тот код специфичен `#parks`-аннотированным sync-примитивам).

## Связанные документы

- [`datetime.md`](datetime.md) — гражданское (календарное) время (Plan 175.1).
- [D316](../spec/decisions/04-effects.md#d316) — `Time`-эффект + amend'ы.
- [D317](../spec/decisions/04-effects.md#d317) — overflow-policy.
- [D318](../spec/decisions/04-effects.md#d318) — Monotonic non-regression.
- [D124](../spec/decisions/06-concurrency.md#d124-monotonic-vs-timestamp--раздельные-типы-для-wall-clock-и-монотонных-часов) — wall/monotonic separation + amend.
- [D237](../spec/decisions/02-types.md#d237-protocol-naming-convention-method-name-capitalized-plan-137-2026-06-09) — Display/Debug naming + amend.
