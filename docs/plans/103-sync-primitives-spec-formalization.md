// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 103 — формализация `std.runtime.sync` примитивов в spec

> **Статус:** 📋 proposed 2026-05-25, не начат
> **Приоритет:** P1 — spec-drift closure, не блокирует релиз; но без
> формализации D-блока публичный контракт API не зафиксирован.
> **Оценка:** малая (~1–1.5 дня), только документация и пересборка spec.
> **Зависимости:** Plan 18 Шаг 1 ✅ (shipped sync.nv/sync_primitives.h),
> Plan 21 ✅ (Channel revision), Plan 83.3 ✅ (Blocking).
> **Связь:** Plan 18 (stdlib roadmap, обновляется этим планом).

## Зачем

В `std/runtime/sync.nv` уже зашиплены production-grade `AtomicInt`,
`AtomicBool`, `Mutex`, `WaitGroup`, `Once` (Plan 18 Шаг 1) — со
`#stable(since = "0.1")` маркерами на каждом API. C-impl в
`compiler-codegen/nova_rt/sync_primitives.h` (fair-FIFO mutex, fiber
park/wake через `nova_sched`, acq/rel memory ordering).

Spec в трёх разных местах говорит **противоречивое**:

| Место | Что говорит |
|---|---|
| `06-concurrency.md` §1531–1597 «Mutex / Atomic — НЕ в spec» | «нижнеуровневые, легко misuse, не AI-friendly; owner-actor pattern закрывает 99% use case'ов» |
| `06-concurrency.md` §641 «Открытые вопросы к D50» | «`Mutex`/`Atomic` отвергнуты в пользу channel-only модели» |
| `open-questions.md` Q12.4 v1.0-плана (§396) | «`Mutex`, `RwLock`, `Atomic[T]` как stdlib-типы (Q12.4)» — **запланировано** |
| `open-questions.md` Q9 (§274) | «Точные API `Mutex`, `RwLock`, `Atomic[T]` — открыто» |
| `04-effects.md` §4302–4304 | использует `Atomic[int].new(0)` как canonical example |

То есть Plan 18 Шаг 1 — это **выполнение обещания Q12.4 v1.0-плана**, а
не нарушение спеки. Но §1531–1597 и §641 остались как устаревшие
декларации «не делаем». Plan 103 закрывает этот drift и фиксирует
точный API.

Без D-блока:
- публичный API `AtomicInt`/`Mutex`/`WaitGroup`/`Once` не имеет
  spec-якоря — будущие изменения не имеют процесса согласования;
- LSP / `nova info` не могут показать «stable since 0.1» с ссылкой;
- противоречие в spec'е путает читателя (что верно — §1531 «не делаем»
  или Q12.4 «делаем»?).

## Scope

### In scope

- **Новый D-блок D167** в `spec/decisions/06-concurrency.md`:
  формализация API `AtomicInt`/`AtomicBool`/`Mutex`/`WaitGroup`/`Once`
  с точными сигнатурами (verbatim из `std/runtime/sync.nv`), memory
  ordering, контракты (fair-FIFO, not-reentrant, exactly-once).
- **Переписать §1531–1597** «Mutex / Atomic — НЕ в spec» как «Mutex
  и Atomic зашиплены через stdlib (см. D167); prelude/owner-actor
  остаётся preferred default».
- **Обновить §641** ссылку «Mutex/Atomic отвергнуты» → «Mutex/Atomic
  — stdlib (D167), не prelude».
- **Закрыть Q9 и Q12.4 в `open-questions.md`** в части
  `Atomic`/`Mutex`/`WaitGroup`/`Once`; `RwLock`/`Semaphore`/`Atomic[T]
  generic` остаются открытыми (см. ниже).
- **Consistency-check:** Nova-side декларации в `sync.nv` точно
  совпадают с C-impl в `sync_primitives.h` (имена методов,
  arity, ordering documented), и оба совпадают с D167.
- **Обновить Plan 18:** Шаг 1 ✅ ЗАКРЫТО (с реальным составом,
  отличается от исходного дизайна — см. ниже), статус Plan 18
  «proposal, не начат» → «частично выполнено, остаток — Plan 91 + 18
  fs/net/http».

### Out of scope

Декомпозируется в отдельные следующие планы:

- **`RwLock`** — не реализован. Plan 18 Шаг 5 «std.sync остаток
  (Once, RwLock, Semaphore)». Once реализован, RwLock и Semaphore —
  нет. Отдельный план, когда возникнет use case.
- **`Semaphore`** — то же.
- **`Atomic[T]` generic** — текущая реализация моно-типы
  (`AtomicInt`/`AtomicBool`), не generic `Atomic[T]`. Generic-версия
  требует Plan 101 `fn[T]` mono fix (`[M-fn-prefix-int-only-mono]`)
  + `Atomic[ptr]`/`Atomic[u64]` отдельные C-impl'ы. Отдельный план.
- **User-configurable memory ordering** (relaxed/seq_cst/etc.) —
  текущая impl жёстко acq_rel/acquire/release. Если возникнет need
  для relaxed (lock-free queues) — отдельный план.
- **Mutex с типизированной guard'ой** (Rust `Mutex<T>` data-carrying
  стиль) — текущий `Mutex` Go-style (без `T`). Решение пользователя:
  фиксировать как окончательный дизайн или предусмотреть migration
  path в `Mutex[T]`. Отдельный design Q.
- **Реализация D167 в коде** — Plan 103 только документирует уже
  существующую реализацию. Если в Ф.4 consistency-check выявит
  расхождения impl↔doc — fix-ы делаются inline (мелкие), либо
  декомпозируются.
- **Всё остальное в `std/`** (concurrency/cancellation, rate_limiter,
  retry, timer; identifiers/ulid/uuid/snowflake; crypto/*;
  encoding/*; data/sql; и т.д.) — отдельный аудит «есть в std, нет
  в spec» (Plan 103 candidate, не начат).

## Фазы

### Ф.0 — Pre-audit consistency (~0.3д)

Перед написанием D167 проверить, что Nova-side и C-side не разошлись:

- **Ф.0.1** Сравнить сигнатуры в `std/runtime/sync.nv`
  (`AtomicInt @load -> int` и т.д.) с inline C
  `Nova_AtomicInt_method_load(const Nova_AtomicInt* a) -> nova_int` в
  `compiler-codegen/nova_rt/sync_primitives.h`. Для каждого метода —
  arity, types, mut-receiver, return-type. Расхождения → таблица.
- **Ф.0.2** Сравнить memory-ordering claims в doc-comments sync.nv
  (acquire/release/acq_rel) с реальными `__atomic_*` calls в .h.
  Любое несоответствие фиксируется как баг.
- **Ф.0.3** Проверить `#stable(since = "0.1")` маркеры — все ли
  методы помечены (включая `swap`, `compare_exchange`, `try_lock`).
- **Ф.0.4** Decision point: если расхождений нет — переходим к Ф.1.
  Если есть мелкие — fix inline. Если крупные — отдельный
  bug-fix plan, Plan 103 ждёт.

Acceptance: один краткий audit-report (можно в самом Plan 103
appendix), нет open inconsistencies.

### Ф.1 — D167 draft (~0.5д)

Добавить новый D-блок в `spec/decisions/06-concurrency.md` (после D79
Channels — концептуально соседи: D79 = channels, D167 = sync
primitives как complement). Структура — каноническая для D-блоков:

```
## D167. `std.runtime.sync` — fiber-aware sync primitives (Plan 18 Шаг 1)

### Что
- `AtomicInt`, `AtomicBool`, `Mutex`, `WaitGroup`, `Once`
- Module: `runtime.sync` (под std/runtime/, не prelude)
- Все API: `#stable(since = "0.1")`

### Правило
[точные сигнатуры verbatim из sync.nv для каждого типа]

### Memory ordering
- Atomic load: acquire
- Atomic store: release
- Atomic fetch_add/sub/swap/compare_exchange: acq_rel
- Once.run() fast path: acquire-load (DONE → terminal)
- Once.done(): release-store

### Контракты (runtime-invariants)
- Mutex: NOT reentrant, fair FIFO, ownership transfer на unlock
- WaitGroup: add() happens-before wait() (Go-style)
- Once: run() returns true ⇒ done() MUST be called exactly once
- Non-fiber path (slot < 0): spin с CPU yield hint вместо park

### Почему
- Closes Q12.4 v1.0-plan (stdlib-типы) и Q9 (точные API)
- Owner-actor pattern (D79) остаётся preferred default — D167
  для случаев когда actor избыточен (perf-critical counters,
  one-shot ownership)
- Implementation: Plan 18 Шаг 1; C-impl
  `compiler-codegen/nova_rt/sync_primitives.h`; Nova-side
  декларации `std/runtime/sync.nv`

### Что отвергнуто (для V1)
- `Atomic[T]` generic — пока моно-типы (AtomicInt/Bool)
- `Mutex[T]` data-carrying (Rust-style) — выбран Go-style без `T`
- `RwLock`, `Semaphore` — отложены до use case
- User-configurable memory ordering — жёстко acq_rel/acquire/release

### Связь
- D79 (Channels) — preferred default; D167 — escape hatch
- D14, D50, D71 — fiber-runtime подложка (park/wake API)
- Plan 18 Шаг 1 — реализация
- Plan 103 — этот план (формализация)
```

Acceptance: D167 в `06-concurrency.md`, `grep "^## D167"` находит.

### Ф.2 — переписать §1531–1597 «НЕ в spec» (~0.2д)

Секция `#### Mutex / Atomic — НЕ в spec` устарела относительно
Q12.4 v1.0-плана и Plan 18 Шаг 1. Переписать:

- **Заголовок:** `#### Mutex / Atomic — stdlib, не prelude (D167)`
- **Тело:**
  - Сохранить аргументацию «owner-actor pattern закрывает 99% use
    cases» — это всё ещё correct guidance.
  - Заменить «не в spec» → «доступны через `std.runtime.sync` (см.
    D167); НЕ в prelude — пользователь делает explicit `import
    runtime.sync.{Mutex, AtomicInt}` для получения escape hatch».
  - Сохранить counter_actor example как preferred default.
  - Добавить параграф «когда D167-примитивы оправданы»:
    perf-critical counters в hot path, one-shot ownership через
    `AtomicBool.swap`, exactly-once init через `Once`.
- Также §1594 «Что отвергнуто: Mutex / Atomic в prelude» — оставить
  как есть (в prelude они и правда не попали), но добавить ссылку
  «но доступны через `std.runtime.sync` (D167)».
- §641 «Mutex/Atomic отвергнуты» в `### Открытые вопросы` D50 —
  заменить на «`Mutex`/`Atomic`/`WaitGroup`/`Once` — stdlib через
  `std.runtime.sync` (D167); `RwLock`/`Semaphore`/`Atomic[T] generic`
  остаются открытыми».

Acceptance: `grep "Mutex / Atomic — НЕ в spec" spec/` → no results;
переписанная секция cross-references D167.

### Ф.3 — open-questions.md cleanup (~0.2д)

- **Q9** (§274 «Точные API Channel[T], Mutex, RwLock, Atomic[T]»):
  - Channel[T] — уже закрыт D79 (отметить если не отмечено).
  - Mutex, AtomicInt/Bool — закрыты D167 (новая ссылка).
  - RwLock, Atomic[T] generic — остаются открытыми с пометкой
    «Plan 18 Шаг 5 / отдельные планы».
- **Q12.4** (§336–347 + v1.0-план §396):
  - В резюме §272 («Остаётся открытым в Q9») — обновить:
    `Mutex`/`AtomicInt`/`AtomicBool`/`WaitGroup`/`Once` зашиплены
    через D167; `RwLock`/`Atomic[T] generic` остаются.
  - В v1.0-plan §396 — пометить пункт «Mutex, RwLock, Atomic[T] как
    stdlib-типы» как ✅ partial (Mutex + AtomicInt/Bool; RwLock/
    generic — pending).
- **Q12.6** уже закрыт `blocking { }` примитивом (Plan 83.3) — не
  трогаем (вне scope Plan 103).

Acceptance: Q9 / Q12.4 в open-questions.md имеют explicit статус
«частично закрыт через D167», с ссылкой на конкретные типы.

### Ф.4 — Update Plan 18 (~0.2д)

- **Заголовок Plan 18:** баннер «DRAFT, не финализирован» →
  «PARTIALLY ACTIVE — Шаг 1 ✅ закрыт через D167 (Plan 103), Шаги
  2-4 → Plan 91 (std MVP), Шаг 5 (RwLock/Semaphore) — отдельные
  планы по запросу».
- **Шаг 1 — std.sync:** добавить статус-блок:
  ```
  > ✅ ЗАКРЫТ Plan 103 (D167, YYYY-MM-DD).
  > Реальный состав отличается от исходного дизайна:
  > - `AtomicInt` + `AtomicBool` (моно-типы), не `Atomic[T]` generic
  > - `Mutex` без `T` (Go-style), не `Mutex[T]` data-carrying
  > - `WaitGroup`, `Once` — как планировалось
  > - `RwLock`, `Semaphore` — НЕ реализованы (Шаг 5)
  ```
- **Шаги 2-4:** добавить пометки «→ Plan 91 (std MVP)».
- **Шаг 5:** оставить «отложено до use case».
- **Связи:** добавить «[103](103-sync-primitives-spec-formalization.md)
  — формализация Шаг 1».

Acceptance: Plan 18 row отражает реальное состояние; нет
сообщения «proposal, не начат» если Шаг 1 фактически зашиплен.

### Ф.5 — plans/README.md (~0.1д)

- Row Plan 18: статус с «proposal, не начат» → «частично закрыт
  (Шаг 1 ✅ через Plan 103; Шаги 2-4 → Plan 91; Шаг 5 deferred)».
- Добавить row Plan 103 → «✅ ЗАКРЫТ YYYY-MM-DD (D167)» (с пометкой что номер 102 зарезервирован под Q-representation-bound из Plan 101).

Acceptance: README.md консистентен с Plan 18 / Plan 103 статусами.

### Ф.6 — Logs + close (~0.2д)

- `docs/project-creation.txt` — раздел `## Plan 103 — sync
  primitives spec formalization` с кратким описанием, D-блок,
  cross-references.
- `nova-private/discussion-log.md` — нарратив-лог `## YYYY-MM-DD —
  Plan 103 closure`: что выяснили (внутренняя противоречивость
  spec), как закрыли (D167 + переписывание §1531 + open-questions
  cleanup), уроки.
- `docs/simplifications.md` — если есть marker `[M-sync-not-in-spec]`
  или подобный — закрыть. Если нет — не создавать.
- Закрыть Plan 103 (статус: ✅ ЗАКРЫТ YYYY-MM-DD).

## Acceptance criteria

- [ ] D167 в `spec/decisions/06-concurrency.md`; `grep "^## D167"` находит.
- [ ] D167 содержит verbatim API из `std/runtime/sync.nv` для всех 5
      типов (AtomicInt/AtomicBool/Mutex/WaitGroup/Once).
- [ ] Memory ordering для каждой atomic-операции явно зафиксирован
      в D167 (acquire/release/acq_rel).
- [ ] Контракты (NOT reentrant Mutex, fair FIFO, exactly-once Once,
      add-before-wait WaitGroup) явно перечислены в D167.
- [ ] `grep "Mutex / Atomic — НЕ в spec" spec/` → нет результатов.
- [ ] Секция в §1531 переписана и cross-reference'ит D167.
- [ ] §641 (D50 «Открытые вопросы») cross-reference'ит D167.
- [ ] Q9 / Q12.4 в `open-questions.md` имеют статус «частично закрыт
      через D167» с явным списком закрытого vs остающегося.
- [ ] Plan 18 баннер DRAFT снят / заменён, Шаг 1 помечен ✅, есть
      ссылка на Plan 103.
- [ ] plans/README.md обновлён: Plan 18 row + новый Plan 103 row.
- [ ] Ф.0 audit-report показал 0 расхождений между sync.nv ↔
      sync_primitives.h ↔ D167 (или расхождения зафиксированы и
      исправлены).
- [ ] `nova test` без новых FAIL (Plan 103 — только документация и
      возможно мелкие fix'ы в Ф.0; полная suite не должна сдвинуться).

## Non-acceptance

- Plan 103 НЕ реализует `RwLock` / `Semaphore` / `Atomic[T]` generic
  — это отдельные планы.
- Plan 103 НЕ переделывает существующий C-impl. Если Ф.0 находит
  баги (например, неправильное memory ordering) — fix inline для
  мелких, отдельный plan для крупных.
- Plan 103 НЕ трогает другие std-модули (concurrency/, crypto/,
  encoding/, identifiers/) — это отдельный аудит (Plan 103 candidate).

## Риски и mitigations

- **Риск:** Ф.0 находит крупные расхождения impl↔doc → Plan 103
  блокируется bug-fix-плана. **Mitigation:** Ф.0 первый, decision
  point — если risk realises, делаем bug-fix отдельным планом, D167
  draft'им под уже-исправленную реальность.
- **Риск:** пользователь хочет другую формулировку D167 (например,
  Mutex[T] data-carrying как future-direction). **Mitigation:** Ф.1
  draft → обсуждение → ревизия draft. D-блок легко правится до
  merge'а.
- **Риск:** spec'овая дискуссия про owner-actor vs Mutex в §1531
  затянет фазу. **Mitigation:** сохраняем существующую аргументацию
  (она correct), только добавляем «escape hatch через stdlib».

## Связь с другими планами

- [18-stdlib-roadmap.md](18-stdlib-roadmap.md) — Plan 18 Шаг 1
  реализован; Plan 103 формализует. После закрытия — Plan 18
  partial-active.
- [21-channel-revision-implementation.md](21-channel-revision-implementation.md)
  — Channel формализован D79 (Channels — preferred default).
- [83.3-blocking-libuv-threadpool.md](83.3-blocking-libuv-threadpool.md)
  — Blocking-эффект (D50 §4); D167 sync primitives — orthogonal.
- [91-stdlib-mvp-for-0.1.md](91-stdlib-mvp-for-0.1.md) — std MVP;
  sync не входит в MVP (см. Plan 91 Non-scope), но Plan 103 закрывает
  spec-долг для уже зашипленного.
- **Будущий план — std-vs-spec audit** — полный аудит std/ vs spec'а
  (concurrency/cancellation, identifiers/ulid, crypto/*, encoding/*,
  text/regex и т.д.). Не входит в Plan 103 scope.
