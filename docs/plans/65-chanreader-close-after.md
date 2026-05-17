// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 65: `ChanReader.close_after(Duration)` — миграция `Time.after`

> **Создан:** 2026-05-18. **Статус:** proposed, не начат.
> **Приоритет:** P1 (исправление спека/API drift; блокер для future
> Channel-based timer API: `tick_every`, `close_at`).
> **Трудоёмкость:** ~5 dev-days.

---

## Зачем

Текущий API `Time.after(ms i64) -> ChanReader[()]` нарушает три инварианта:

1. **Domain vs return type mismatch.** Функция живёт в `Time`, но
   возвращает только read-capability канала. Discovery плохой: чтобы
   найти «как сделать канал с таймером», надо сначала угадать что искать
   надо в `Time`, а не в `Channel`/`ChanReader`.

2. **Нет type safety на длительности.** `Time.after(1000)` — это 1 сек
   или 1000 микросекунд? Bare int не несёт единицу измерения.
   `Duration.from_secs(1)` явный — `Time.after` его не принимает.
   Дублируется паттерн ms-как-int (был в Plan 22 Time.sleep до Duration
   migration; тут эта симплификация осталась).

3. **Semantic mismatch с capability-split D91.** Получение
   `ChanReader[()]` через `Time.<X>` подразумевает что Time владеет
   также `ChanWriter` (который остался у runtime). Это нарушает
   ментальную модель D91: namespace = capability, которую получает
   caller.

**Real impact:**
- 16 call sites в `nova_tests/concurrency/*` (7 файлов) с raw int ms —
  все они риск ошибки timeline (мс vs сек).
- spec D94 examples используют bare int — учит неправильному паттерну.
- Плотно использован в `select`-based timeout pattern (главный use case).

**Что уже сделано** (контекст):
- D91 capability-split реализован (Plan 21).
- Duration API готов: `from_secs/from_millis/from_nanos/from_secs_f64/...`
  (std/time/duration.nv).
- libuv timer infrastructure готов (Plan 22, Plan 44.1 Ф.2 timer cleanup).
- Plan 60 показал production-grade pattern atomic migration tool +
  spec sync (D117 size-accessor uniformity).

---

## Текущее состояние (verified 2026-05-18)

| Слой | Где | Что |
|---|---|---|
| Spec | `spec/decisions/06-concurrency.md` D94 + 5+ упоминаний | `Time.after(d)` примеры |
| Compiler schema | `emit_c.rs:1042-1046` | `time_schema.insert("after", ([i64], "Nova_ChanReader*"))` |
| Codegen type inference | `emit_c.rs:18195-18198` | Member-call `Time.after` → `Nova_ChanReader*` |
| Runtime | `nova_rt.h` / channel impl | `nova_time_after_ms(int64)` extern |
| Tests | 7 файлов в `nova_tests/concurrency/` | 16 call sites с ms-int literals |
| Plan doc | Plan 31 Ф.5, Plan 44.1 Ф.2 B7 | Historical reference |

---

## Архитектурное решение

### AD1. New API: `ChanReader.close_after(Duration) -> ChanReader[()]`

```nova
let t = ChanReader.close_after(Duration.from_secs(1))
select {
    Some(v) = rx.recv() => process(v)
    None    = t.recv()  => log_idle()
}
```

**Свойства:**
- Namespace `ChanReader` = capability возврата (D91 consistency).
- Метод `close_after` описывает **механизм** (канал закроется через d),
  совпадает с pattern `None = t.recv() =>` в select arm.
- Параметр `Duration` — type-safe, unit-explicit.

### AD2. Старый `Time.after` удаляется (clean break)

Bootstrap convention (precedent: Plan 60, Plan 47 D75 revision): API
переименовывается atomically, старое имя удаляется в том же sweep'е.
Diagnostic с structured fix-it suggestion ловит legacy code.

**Почему не deprecated alias.** Поддерживать оба варианта = два path в
codegen + risk drift + confusion в documentation. Bootstrap 0.x — нет
backward-compat обязательств.

### AD3. Duration → ns в runtime, не ms

Existing runtime `nova_time_after_ms(int64)` принимает миллисекунды.
Это **симплификация** — теряется sub-ms precision (для bench, retry,
fine-grained scheduling).

**Решение:** новая runtime fn `nova_chan_reader_close_after_ns(int64)`,
принимает nanos. Внутри libuv timer всё равно работает на ms (libuv
ограничение), но **округление вверх** документировано и происходит в
одной точке (runtime), не в каждом call site.

```c
// nova_rt.c
Nova_ChanReader* nova_chan_reader_close_after_ns(int64_t nanos) {
    // libuv ограничение: ms granularity → округление вверх
    uint64_t ms = (nanos + 999999) / 1000000;
    if (ms == 0 && nanos > 0) ms = 1;  // sub-ms → 1ms minimum
    return nova_libuv_timer_close_after(ms);
}
```

**Audit-finding to fix in plan:** sub-ms duration в текущем `Time.after`
silently округляется до 0, что = «сработать сразу же». Это bug, не
feature.

### AD4. Codegen: Duration unpacking inline

`Duration` — record `{ nanos i64 }`. Codegen для `ChanReader.close_after(d)`
эмитит:

```c
nova_chan_reader_close_after_ns(d.nanos)
```

(без allocation промежуточной структуры; field-access прямой).

**Edge case:** literal `Duration.from_secs(1)` constant-folded → emit
hard-coded `nova_chan_reader_close_after_ns(1000000000LL)`. Optimization
в Plan 65 Ф.8 (perf-sanity).

### AD5. Negative-test scope (production-grade)

| Test | EXPECT marker | Что проверяет |
|---|---|---|
| `f1_close_after_int_neg.nv` | `EXPECT_COMPILE_ERROR` | `ChanReader.close_after(1000)` — int не Duration |
| `f2_time_after_removed.nv` | `EXPECT_COMPILE_ERROR` | `Time.after(1000)` — removed |
| `f3_negative_duration.nv` | `EXPECT_RUNTIME_PANIC` | `Duration.from_secs(-1)` → close_after — panic «negative duration» |
| `f4_zero_duration.nv` | (positive) | `Duration.from_nanos(0)` → канал готов immediate (recv returns None ASAP) |
| `f5_sub_ms_precision.nv` | (positive) | `Duration.from_nanos(500_000)` (0.5ms) → округляется к 1ms, не к 0 |
| `f6_huge_duration.nv` | `EXPECT_COMPILE_ERROR` или runtime warn | `Duration.from_days(10_000)` overflow check |

### AD6. Migration tool — атомарный, idempotent

Rust binary `nova-cli/src/bin/migrate_plan65.rs` (паттерн Plan 60
`migrate_plan60.rs`):

- **Token-aware**: парсит .nv с lexer (не regex по тексту), чтобы не
  ломать строки/комментарии содержащие `Time.after`.
- **Транформации**:
  - `Time.after(<INT_LIT>)` → `ChanReader.close_after(Duration.from_millis(<INT_LIT>))`
  - `Time.after(<FLOAT_LIT>)` → `ChanReader.close_after(Duration.from_secs_f64(<FLOAT_LIT>))`
  - `Time.after(<EXPR>)` (non-literal) → emit marker comment
    `// MIGRATE_MANUAL: Plan 65 — wrap in Duration.from_millis(<EXPR>)`
- **Idempotent**: повторный запуск на уже-мигрированном файле — no-op.
- **Dry-run mode** (`--dry-run`): print planned changes без записи.
- **Exit code 1** если manual markers остались — CI gate.

---

## Requirements

### Core API (MVP)

**R1.** `ChanReader.close_after(d Duration) -> ChanReader[()]` доступен
как static constructor на типе `ChanReader`. Может быть вызван без
import (через prelude расширение если ChanReader там есть, иначе через
`std/concurrency/channel`).

**R2.** Принимает только `Duration`. `ChanReader.close_after(1000)`
(int literal) → compile error с suggestion «use `Duration.from_millis(1000)`».

**R3.** Семантика: канал closing **по истечении** d (не «через d или
позже» — guaranteed monotonic deadline). recv() вернёт `None` единожды
(стандартное закрытие); повторный recv → `None` (idempotent на closed).

**R4.** Negative Duration → panic «`ChanReader.close_after: negative duration`»
с указанием call site. Не silent zero-out.

**R5.** Zero Duration (`Duration.from_nanos(0)`) → канал closed
**immediately** (next recv returns None без yield). NOT panic — это
valid degenerate case для fast-path testing.

**R6.** Sub-ms duration → округление вверх к 1ms (libuv ограничение,
не lying-zero). Документировано в `///` doc-comment.

### Compiler

**R7.** Type-checker: registration `ChanReader.close_after` через
external_registry (как Channel.new, etc.).

**R8.** Codegen `emit_c.rs`:
   - `infer_expr_c_type` case: `ChanReader.close_after(...)` → `Nova_ChanReader*`
   - Method dispatch lowering: emit `nova_chan_reader_close_after_ns(d.nanos)`

**R9.** `Time.after(...)` диагностика: structured `Diagnostic` с
`SuggestedFix` (Plan 36 R7 infrastructure). Машино-applicable: caller
видит точную замену.

```
error[E5101]: `Time.after` was removed in Plan 65
  --> nova_tests/concurrency/select_test.nv:194:33
   |
194|     Some(_) = Time.after(50) => { branch = 2 }
   |               ^^^^^^^^^^^^^^^ use `ChanReader.close_after(Duration)` instead
   |
   = note: `Time.after(ms)` accepted raw integers — no unit safety.
   = note: `ChanReader.close_after(Duration)` is the capability-split
           replacement (D91, D94 revision).
help: replace with `ChanReader.close_after(Duration.from_millis(50))`
  --> nova_tests/concurrency/select_test.nv:194:33
   |
194|     Some(_) = ChanReader.close_after(Duration.from_millis(50)) => { branch = 2 }
   |
```

### Runtime

**R10.** New extern: `nova_chan_reader_close_after_ns(int64_t nanos) -> Nova_ChanReader*`.

**R11.** Existing `nova_time_after_ms` — переименовать в
`nova_internal_chan_close_after_ms` (internal helper); вызывается из
`nova_chan_reader_close_after_ns` после ns→ms conversion. Не доступен
из user code.

**R12.** Timer cleanup contract preserved (Plan 44.1 Ф.2 B7): non-winning
arm в select cancel'ит pending timer. Existing `on_select_lost` callback
remains.

### Migration

**R13.** Migration tool processes std/, nova_tests/, examples/.
Spec/ — manual (D94 amendments в Ф.6).

**R14.** Post-migration verification:
   - `grep -rn "Time\.after" std/ nova_tests/ examples/` → 0 hits
   - `nova test` → 0 regressions (baseline same as pre-migration)

### Documentation

**R15.** `///` doc-comments на `ChanReader.close_after`:
   - One-line summary
   - `# Examples` block с select-pattern
   - `# Errors` block: negative Duration panic
   - `# Performance` note: libuv timer ms-granularity rounding
   - `#stable(since = "0.6")` badge

**R16.** doc-test через `nova doc --test` (Plan 45 Ф.7):
   - Positive: select с close_after, recv возвращает None после d
   - Negative compile: bare int → diagnostic

### Cross-toolchain

**R17.** Build + test pass на Clang/MSVC/GCC (Plan 58 matrix). Critical:
runtime ns→ms conversion uses portable arithmetic (no compiler-specific
int128, etc.).

---

## Phases

### Ф.0 — Audit baseline (½ day)

- [ ] `nova test` baseline на main — fix exact PASS/FAIL count.
- [ ] `grep -rn "Time\.after" --include="*.nv" std/ nova_tests/ examples/`
      — записать список (expected ~16 в 7 файлах).
- [ ] `grep -rn "Time\.after" --include="*.md" spec/ docs/` — записать
      манually-edit list.
- [ ] Audit runtime extern: confirm `nova_time_after_ms` signature
      (в `nova_rt.h` или `channel.c`).
- [ ] Confirm `Duration` API completeness (нужны: from_secs / from_millis /
      from_micros / from_nanos / from_secs_f64).
- [ ] Document baseline в `docs/plans/65-artifacts/baseline-2026-05-XX.md`.

**Acceptance:** baseline.md с counts + file list.

### Ф.1 — Compiler registration (1 day)

- [ ] Add `ChanReader.close_after(d Duration) -> Nova_ChanReader*` в
      `external_registry.rs` (или эквивалент). Pattern: copy `Channel.new`
      registration, adapt for static method on `ChanReader`.
- [ ] `infer_expr_c_type`: add Member case `ChanReader.close_after` →
      `Nova_ChanReader*`.
- [ ] Verify type-checker accepts new call site (smoke test:
      `let _ = ChanReader.close_after(Duration.from_secs(1))`).
- [ ] **Не** удалять Time.after registration yet — будет в Ф.5 atomic switch.

**Acceptance:** new API type-checks; smoke test compiles.

### Ф.2 — Runtime layer (½ day)

- [ ] Implement `nova_chan_reader_close_after_ns(int64_t)` в runtime
      (`compiler-codegen/c-runtime/channel.c` или эквивалент). Конвертация
      ns→ms с round-up + sub-ms minimum 1ms.
- [ ] Rename internal `nova_time_after_ms` → `nova_internal_chan_close_after_ms`
      (signal что это helper, не user-facing API).
- [ ] Codegen emit: `ChanReader.close_after(d)` → C call
      `nova_chan_reader_close_after_ns(d.nanos)`.
- [ ] Negative Duration check: `nanos < 0` → panic «`ChanReader.close_after: negative duration: -N ns`».
- [ ] Zero Duration: immediate-close branch (без libuv timer alloc).
- [ ] Unit-test runtime функцию в isolation (sub-ms, zero, negative,
      large values).

**Acceptance:** runtime функция handles 4 corner cases (negative/zero/
sub-ms/normal); existing select_timer_* тесты PASS под new runtime
после Ф.5.

### Ф.3 — Negative-test fixtures (½ day)

- [ ] `nova_tests/plan65/f1_close_after_int_neg.nv` — `EXPECT_COMPILE_ERROR`
      with structured suggestion match.
- [ ] `nova_tests/plan65/f3_negative_duration.nv` — `EXPECT_RUNTIME_PANIC`
      «negative duration» substring.
- [ ] `nova_tests/plan65/f4_zero_duration.nv` — positive: канал closed
      immediately, recv returns None в same fiber tick.
- [ ] `nova_tests/plan65/f5_sub_ms_precision.nv` — positive: 500_000 ns
      округляется к 1ms (verified via Time.now() pre/post recv).
- [ ] `nova_tests/plan65/f6_huge_duration.nv` — verify either compile
      error или runtime overflow handle (depends on libuv timer max).

**Acceptance:** 5 tests PASS под new API.

### Ф.4 — Migration tool (1 day)

- [ ] Implement `nova-cli/src/bin/migrate_plan65.rs`:
   - Use existing lexer от nova-codegen (token-aware).
   - Transform rules (R-table выше).
   - Dry-run mode + exit code 1 если manual markers.
- [ ] Test migration tool на specific files:
   - `nova_tests/concurrency/select_test.nv` (3 simple int literals).
   - `nova_tests/concurrency/plan40_channel_hardening.nv` (4 calls incl.
     potentially computed).
- [ ] Idempotency test: run twice — no diff between runs.
- [ ] Bin registered в `nova-cli/Cargo.toml`.

**Acceptance:** tool migrates 16/16 call sites; idempotent.

### Ф.5 — Atomic switch (½ day)

- [ ] Run migration tool на std/ + nova_tests/ + examples/ (single
      commit для migration).
- [ ] Remove `Time.after` registration из compiler `time_schema`.
- [ ] Remove infer_expr_c_type case для `Time.after`.
- [ ] Add type-checker diagnostic для `Time.after(...)` с suggested fix
      (R9 format).
- [ ] `nova test` → 0 regressions vs Ф.0 baseline.
- [ ] Verify `grep -rn "Time\.after"` в migrated файлах = 0.

**Acceptance:** baseline test count preserved; legacy diagnostic ловит
любой residual call site.

### Ф.6 — Spec sync (½ day)

- [ ] `spec/decisions/06-concurrency.md` D94:
   - Replace 4 inline examples `Time.after(...)` → `ChanReader.close_after(Duration.from_*)`.
   - Add new sub-section «### Эволюция»: «2026-05-XX (Plan 65): renamed
     `Time.after(ms)` → `ChanReader.close_after(Duration)`. Rationale:
     D91 capability semantics + Duration type-safety.»
   - Update D94 TOC line (line 18) с новой формулировкой.
- [ ] `spec/decisions/06-concurrency.md` other sections (line 1318, 1338,
      2701-2713 etc.) — same migration.
- [ ] Update D-block timestamp note.
- [ ] Cross-check `docs/plans/31-channel-select.md`, `44.1-channel-hardening.md`
      — add «**Note (Plan 65):** historical reference; current API is
      `ChanReader.close_after(Duration)`.» (не переписывать plan-doc — это
      historical record).
- [ ] Update `spec/decisions/README.md` index если D94 entry text менялся.

**Acceptance:** spec examples все используют new API; эволюция
documented; нет dangling reference на `Time.after`.

### Ф.7 — Stdlib documentation (½ day)

- [ ] Найти где `ChanReader` definition (std/concurrency/channel.nv?).
      Add `export fn ChanReader.close_after(d Duration) -> Self`
      stub-declaration с full doc-comment (даже если registration в
      compiler) — для discoverability через nova doc.
- [ ] `///` doc-comment:

```nova
/// Создаёт `ChanReader[()]`, который закрывается через указанную длительность.
///
/// Используется в `select` как timeout-arm: `None = t.recv() => { ... }`
/// сработает после истечения `d`.
///
/// # Examples
///
/// ```nova
/// let t = ChanReader.close_after(Duration.from_secs(1))
/// select {
///     Some(v) = rx.recv() => process(v)
///     None    = t.recv()  => log_timeout()
/// }
/// ```
///
/// # Errors
///
/// Panics if `d` is negative.
///
/// # Performance
///
/// Internally backed by libuv timer (1ms granularity). Sub-millisecond
/// durations round up to 1ms. Use `Duration.from_nanos(0)` for
/// immediate close (no timer allocation).
///
/// # Связь
///
/// - [D91](../../spec/decisions/06-concurrency.md#d91) — capability-split.
/// - [D94](../../spec/decisions/06-concurrency.md#d94) — select syntax.
#stable(since = "0.6")
export fn ChanReader.close_after(d Duration) -> Self
```

- [ ] doc-test (Plan 45 Ф.7): execute embedded example, verify behaviour.
- [ ] Run `nova doc --check std/` — no broken links.

**Acceptance:** doc-test PASS; `nova doc` renders без warnings.

### Ф.8 — Cross-toolchain + perf-sanity (½ day)

- [ ] Build matrix: Clang/MSVC/GCC (Plan 58 infrastructure).
- [ ] Run `nova test` на каждом backend — preserve baseline.
- [ ] Perf-sanity: `select_timer_stress.nv` (500 iter) на MSVC + Clang
      — no leak, no перформанс-регрессия > 5% vs baseline.
- [ ] Bench (Plan 57): `bench "close_after_alloc" { measure { let _ = ChanReader.close_after(Duration.from_millis(10)) } }` — записать в history. Не gate (нет prior baseline для close_after specifically), просто snapshot.

**Acceptance:** все toolchain PASS; perf snapshot recorded.

### Ф.9 — Project docs + cleanup (½ day)

- [ ] `docs/project-creation.txt`: add 2026-05-XX entry: «Plan 65 closed».
- [ ] `docs/simplifications.md`:
   - Mark `[M-time-after-bare-int]` (если был) как RESOLVED.
   - Add `[M-libuv-ms-granularity]` honest-note: sub-ms rounded up, не
     true ns precision.
- [ ] Bench-history sync (`bench history-add` если есть baseline change).

**Acceptance:** project-creation.txt updated; simplifications.md
honest о ms granularity tradeoff.

---

## Acceptance criteria (production-grade)

- [ ] `grep -rn "Time\.after" std/ nova_tests/ examples/` → **0 hits**
      (после Ф.5).
- [ ] `grep -rn "Time\.after" spec/ docs/plans/` — только в historical
      эволюция notes (D94 «Эволюция» section + Plan 31/44.1 historical
      reference markers).
- [ ] `ChanReader.close_after(Duration)` accepts только Duration —
      `EXPECT_COMPILE_ERROR` test для bare int passes.
- [ ] `Time.after(...)` → structured diagnostic с machine-applicable
      suggestion (R9 format).
- [ ] `nova test` (release) — 0 regressions vs Ф.0 baseline (562 PASS /
      0 FAIL preservation **as of plan creation date**; adjusted к
      current baseline at execution time).
- [ ] Cross-toolchain: PASS на Clang (default), MSVC, GCC.
- [ ] D94 spec amended; примеры new API; «Эволюция» note.
- [ ] doc-test (`nova doc --test std/concurrency/`) PASS для close_after.
- [ ] Plan 65 migration tool idempotent (re-run = no diff).

---

## Open questions

1. **`ChanReader.close_at(Timestamp)` — добавлять сразу?**
   - Use case: «timeout at absolute deadline» (вместо «через d»).
   - Symmetric с Tokio's `sleep_until(Instant)`.
   - **Решение:** deferred в Plan 65+1 (separate plan). Plan 65 scope —
     только rename existing.

2. **`ChanReader.tick_every(Duration)` — periodic timer?**
   - Use case: heartbeat, periodic check.
   - Go's `time.Tick`.
   - **Решение:** deferred. Не conflicts с Plan 65, отдельная фича.

3. **Stdlib re-export через prelude?**
   - `ChanReader` уже доступен (D91). Метод `close_after` discoverable
     через `ChanReader.<tab>`.
   - **Решение:** stdlib только; no prelude pollution (Plan 62 territory).

4. **Backward-compat alias?**
   - Bootstrap convention (Plan 60, Plan 47) — clean break.
   - **Решение:** no alias. Diagnostic ловит legacy.

---

## Что НЕ делает (out of scope)

- Не добавляет new timer primitives (`close_at`, `tick_every`).
- Не меняет underlying libuv-based runtime (ms granularity preserved).
- Не реализует true ns precision (HW limitation; honest-defer).
- Не trogает Time.sleep / Time.now / других `Time` methods — только
  `Time.after`.
- Не trogает `Channel.new`/`ChanWriter.send` API.

---

## Связь

- **[D91](../../spec/decisions/06-concurrency.md#d91)** — capability-split
  ChanReader / ChanWriter. Plan 65 — natural extension D91 на static
  constructors.
- **[D94](../../spec/decisions/06-concurrency.md#d94)** — select syntax.
  Plan 65 amends examples (механизм select unchanged).
- **[Plan 31](31-channel-select.md)** — original select implementation
  (Time.after Ф.5). Historical reference.
- **[Plan 44.1 Ф.2 B7](44.1-channel-hardening.md)** — timer cleanup
  contract. Preserved.
- **[Plan 22](22-sleep-libuv-integration.md)** — libuv timer infra.
  Reused.
- **[Plan 60](60-len-access-uniformity.md)** — precedent для atomic
  migration tool + diagnostic-driven rename.
- **[std/time/duration.nv](../../std/time/duration.nv)** — Duration API.
- **[Plan 45 Ф.7](45-nova-doc.md)** — doc-tests infrastructure (для R16).
- **[Plan 58](58-cross-toolchain-msvc-verification.md)** — cross-toolchain
  matrix (для R17).

---

## Эволюция плана

- **2026-05-18 created**: исходный план, P1, 5 dev-days, 10 фаз
  production-grade без симплификаций.
