# Plan 173 — Система ошибок и cleanup: унификация + hardening (panic/fail/defer/on_exit), production-grade

> **Top-level umbrella-план.** Создан 2026-06-20. **Ред. 2 — 2026-07-03**: полная сверка (5-агентный аудит-workflow):
> ground-truth дефектов перепроверен по текущему коду (11/11 живы, строки актуализированы), противоречия
> шапки/§4/§6 с owner-пересмотрами §3a/§3b устранены, планка расширена до **7 языков (+Zig/Swift)**,
> тест-план приведён к test-conventions (одна папка + spec_tests-покрытие), добавлены §7a запуск-чеклист и Ф.0R.
> **Статус:** 📋 READY — Ф.0 закрыта (sign-off 2026-06-20; пересмотры §3a/§3b 2026-06-26 внесены в текст);
> старт = Ф.0R → Ф.1.
> **Маркер:** `[M-173-error-system]`. **Запуск:** «**выполни план 173**» (план самодостаточен; чеклист — §7a).
> **Объединяет** бывш. Plan 174.3 (error-system; NB: номер 174.3 позже переиспользован — ныне это
> [any/is-downcast](174.3-any-type-and-is-downcast.md)) + panics-clause (Ф.6) под номером 173 (2026-06-20).
> **Decision-gate Ф.0 ЗАКРЫТ:** Model 1 (defer-kernel). Supervisor-часть Ф.0-решения **ПЕРЕСМОТРЕНА** дважды
> (2026-06-21: стратегии = эффект-хендлеры, [173.2](173.2-supervision-as-effect.md); 2026-06-26 §3b: restart
> по умолчанию НЕТ — cancel/Escalate/Stop; restart только для изолированных файберов, гейт 173.3).
> **Завершает** [174.2](174.2-question-mark-return-only.md) (`?`-return-only), баги
> `[M-172-with-fail-swallows-panic]`, `[M-172-errdefer-okdefer-dead-surface]`.
> **Источник:** deep-analysis workflow 2026-06-20 (4 агента) + аудит-workflow 2026-07-03 (5 агентов).
> **Хаб:** [docs/idiom/error-and-cleanup-model.md](../idiom/error-and-cleanup-model.md).
>
> **Очередность (граф 173-176 — [README планов §Очередность](README.md), 2026-07-03):** Волна 0 = Ф.0R;
> Волна 1 трек B = Ф.1 → Ф.2 (параллельно трекам 174.3/175/176-io-core). **Входящие гейты:** Ф.3-семейство —
> после Ф.2 (а `deadline:`/`timeout:`-параметры §3a и удаление `with_timeout` — после **Plan 175**); **Ф.4 ←
> 174.3 done** (критический путь); Ф.6 ← Ф.1 + Ф.5-reset. **Исходящие:** Ф.1 разблокирует 174.2-остаток;
> Ф.2 (Cleanup[E]-rename) разблокирует 176 Ф.2 (`File impl Cleanup[IoError]`-мост).
> **ОБЯЗАТЕЛЬНЫЙ сквозной критерий приёмки: «без упрощений, как для прода».** Ни одна фаза не
> закрывается заглушкой/симплификацией; фазирование — это ПОРЯДОК, не урезание объёма. Всё доводится
> до production-grade; «позже» ≠ «опционально».

---

## 1. Зачем (текущая система плохая — ground-truth re-verified 2026-07-03: 11/11 живы, 0 FIXED)

Корень: модель — **ОДИН longjmp-транспорт** (`NovaFailFrame`-цепочка на `_nova_fail_top`,
`effects.h:55-64`), общий для `throw`/typed-throw/`panic`/`cancel`/`assert`/contract/overflow,
различаемый ТОЛЬКО полем `error_kind` (USER/USER_TYPED/CANCEL/PANIC); `interrupt` — отдельный стек
`NovaInterruptFrame`. Поверх — **ТРИ несведённые поверхности** (`defer`, `Consumable.on_exit`/`consume{}`,
`with Fail[E]`), оставшиеся от **двух непримирённых эпох**. Каждый setjmp-кадр **сам** ре-диспатчит по
`error_kind` → дрейф политик, дублированный codegen, и реальные баги.

**Подтверждённые дефекты (file:line актуализированы 2026-07-03):**

| # | Дефект | Где (актуально) | Sev |
|---|--------|-----|-----|
| 1 | `with Fail[E]` **глотает panic** (D13 violation): catch-ветка проверяет только `error_kind == NOVA_THROW_CANCEL` (:6896), PANIC проваливается в USER-path «handler already ran» (:6916-6929) → result=default, выполнение продолжается | `emit_c.rs:6885-6933` | **P1 soundness** |
| 2 | Диагностика `D133-not-consumed` строит quick-fix (Applicability::MaybeIncorrect) с retracted `errdefer`/`okdefer` → код, который парсер реджектит (`parser/mod.rs:10070-10090`) | `types/mod.rs:18764-18798, 18819-18854` + D162-quickfix `:19636-19641` | **P1 user-facing** |
| 3 | `?` Fail-context throw-mode (D165) ещё живёт в codegen (`in_fail_ctx` :21901-21903 → `nova_throw_typed`); `[E_TRY_IN_FAIL_FN]` не существует (grep=0). Plan 174.2 = 📋 PLANNED, не начат | `emit_c.rs:21895-21958` | **P1** |
| 4 | Мёртвый `errdefer`/`okdefer`/`defer\|result\|` surface: `Stmt::ErrDefer/OkDefer` (`ast/mod.rs:1849-1865`), лексер-токены (`lexer/mod.rs:687-688`, `token.rs:145/149/287-288`), `DeferKind` (`emit_c.rs:1322-1343`), недостижимые ветки `emit_c.rs:17912-18280` + `2379-2380, 11704-11705, 19520-19521, 28955-28956` | см. слева | P3 hygiene |
| 5 | `ScopeOutcome.Failure` = `str` в коде vs `any` в спеке → типизированный error-dispatch в `on_exit` невозможен (`nova_make_ScopeOutcome_Failure` берёт только error_msg, payload/type_id дропаются) | `core.nv:147` vs `D188`, `emit_c.rs:19736,19743` | P2 |
| 6 | `MultiError` **никогда не материализуется**: chain write-only (`nv_compose_suppressed` :19839, `nova_rethrow_with_suppressed` :18063/19842/19847/19852); read-аксессоры `nova_failframe_suppressed_count/at` (`effects.h:269-283`) codegen'ом НЕ используются (grep=0); `MultiError` объявлен в `std/prelude/errors.nv:199` с методами `@primary/@suppressed/@walk/@find_first_panic` (:207-250), но никто его не конструирует — D158/D193 обещание не выполнено | `emit_c.rs`, `effects.h`, `errors.nv` | P2 |
| 7 | Suppressed-chain **теряется** на голом `throw`/cancel/typed во время unwind: безусловный `error_suppressed = NULL` | `effects.h:93,114,131,801` + NOVA_TRY `:285` | P2 |
| 8 | D188 R2 exactly-once **runtime-счётчик не реализован** (`_consume_count` есть только в спеке `03-syntax.md:8189`) | — | P2 |
| 9 | `nv_resume_panic` (спека `03-syntax.md:8161`) — фикция; код зовёт `nv_panic` (`effects.h:555`) | `emit_c.rs:19825,19830` | P3 |
| 10 | exit_timeout 3-level — **заглушка** (TODO Level 1/2, хардкод `return 5000`). NB: закрывается **УДАЛЕНИЕМ** force-механизма per §3a п.2 (D192-ретракт), не достройкой — см. Ф.5 п.2 | `effects.h:256-260` | P2 |
| 11 | **Stale спека:** `## D4` жив без RETRACTED-баннера (`04-effects.md:290-304`) + дубль `#### ?` (:950) — NB-врезка у D85 (:4441-4446) уже называет их устаревшими, но секции не переписаны; D90 (`03-syntax.md:4517`) / D158 (:6395) / D161 (:6737) — живые с errdefer-каноном; D160 (:6612) **уже** несёт RETRACTED-баннер (тело всё ещё про okdefer); `spec/decisions/README.md` строки 18/19/36 держат D4/D67/errdefer как live | см. слева | P2 |

**Concurrency-дыры (актуализировано):**

| Конструкция | Текущее | Дыра |
|---|---|---|
| `spawn`/`supervised` | throw ребёнка ловится внешним `with Fail` (scope-гранулярность); panic неперехваченный → abort; первая ошибка → siblings cancel cooperatively (USER beats CANCEL) | `parse_supervised` (`parser/mod.rs:9764-9800`) принимает **только `cancel:`**; supervision-модель — [173.2](173.2-supervision-as-effect.md) (эффект-хендлер, НЕ sum-type-аргумент); **нет per-fiber retention** (first-wins) — [173.0](173.0-concurrency-runtime-substrate.md) |
| `detach` | orphan-fiber: panic/throw = LogAndDrop в stderr | **не перехватываемо вообще**; Detach-эффект не enforced (06-concurrency.md:919). НЕ покрыт 173.0-173.3 → **Ф.3-остаток** |
| `parallel for` | десугар в supervised+spawn (`emit_parallel_for` :8280) | whitelist элемент-типа `:8394-8395` — только `{int,bool,f64,str}`; иначе **молчаливый degrade** в statement-mode (:8396-8411) → утечка сырого C-error (`[M-parfor-record-result-miscompile]`, home [173.1](173.1-parallel-collect-and-supervised-value.md)); **interim-guard `[E_PARFOR_RESULT_UNSUPPORTED]` → Ф.1 п.5** |
| `blocking{}` | block-form ретракнут (D172); V1 leaf не должен throw'ить | ошибки только через `Result`-возврат |
| `channels`/`select` | ошибка канала = ЗНАЧЕНИЕ (`recv→Option`, None = closed+drained) | **РЕШЕНО (Ред.2): recv остаётся `Option`** (канон D91; 173.1-десугар на нём стоит); дефект сводится к select-wildcard семантике (`_ = rx` ловит и value, и closed, `select_closed_test.nv:29`) → верифицировать `None = rx.recv()`-арм D94 + тест + doc → **Ф.3-остаток** |

---

## 2. Планка «не хуже Go/Rust/TS/Kotlin/Java/**Zig/Swift**»

> Ред. 2: планка расширена до **7 языков**. Все упоминания «5 языков» в прежнем тексте читать как «7».

**Nova уже выигрывает (СОХРАНИТЬ как инвариант приёмки; regression-guard тесты обязательны):**
- 3-уровневая таксономия катастроф (panic = смерть **fiber'а**, не процесса; exit = процесс) — чище Go (unrecovered panic убивает процесс), **Zig** (`@panic` = abort, defer/errdefer НЕ бегут) и **Swift** (`fatalError` = abort, defer НЕ бегут).
- **panic ЗАПУСКАЕТ cleanup** (defer + on_exit через fail-frame) — строго лучше Rust (`panic=abort`/double-panic пропускают Drop), Zig и Swift (см. выше).
- `on_exit(ScopeOutcome)` унифицирует Success/Failure/Panic/Cancel в одном exhaustive-match — ни один из 7 языков не выражает это одной конструкцией. **Zig** нужна пара defer+errdefer, «только на успех» невыразимо; **Swift** outcome-биндинга нет вовсе (идиома-костыль `var success = false; defer { if !success … }`).
- **Типизированный payload ошибок:** Zig-ошибка = голый тег error set БЕЗ payload (upstream отклонён, issue #2647; диагностику тащат out-параметром); Nova `Fail[E]` несёт typed payload. Со Swift typed throws (SE-0413) — parity, но у Nova typed-errors первичны, не надстройка над `any Error`.
- **Cleanup-ошибки компонуются, не теряются:** Zig defer/errdefer НЕ может фейлить (`try` в defer = compile-error → ошибки `close()` глотаются); Swift defer НЕ может throw. Nova `@cleanup` может фейлить → suppressed-chain/MultiError (Ф.4) сохраняет И body-, И cleanup-ошибку. Строго лучше обоих.
- **Partial-init cleanup декларативен:** Zig errdefer-in-loop/partial-array-init — известный footgun (ручной индекс-трекинг); Nova multi-binding `consume` (Ф.2 п.6, D188 R1) даёт LIFO+partial-init из вложенности.
- `MultiError` (primary + suppressed[]) — бьёт Go (плоский `errors.Join`), на уровне Java `getSuppressed`.
- keyword-level структурная конкурентность — впереди Go (errgroup) и TS (Promise.all без отмены); **агрегация лучше Swift** (`ThrowingTaskGroup` отменяет siblings, но их ошибки ТЕРЯЕТ — first-wins; Nova → MultiError).
- cancel как структурный `Failure(CancelError)` + shield — принципиальнее Kotlin `CancellationException` **и Swift `CancellationError`** (оба глотаются generic-catch'ем).
- effect-row `Fail[E]` делает множество ошибок видимым + `?`-эргономика; exhaustive match по вариантам E = parity с Zig exhaustive error-set switch.
- §3a-модель «defer всегда добегает» = Swift-модель (валидация выбором Swift), сверх того — outcome-биндинг и композиция ошибок cleanup.

**Nova под риском (ОБЯЗАНЫ закрыть, иначе ХУЖЕ):**
1. **with-Fail глотает panic** — утечка tier-1, хуже ВСЕХ семи (все запрещают ловить unrecoverable как обычный flow). → Ф.1.
2. Дочерняя ошибка в `parallel for`/`supervised` обязана **по умолчанию отменять siblings И агрегировать** (гарантия Kotlin coroutineScope / Java `Joiner`; строго лучше Swift TaskGroup first-wins). → Ф.3-семейство; **порядок**: default = Escalate с per-slot retention (173.0) сразу; проброс primary+suppressed=MultiError — после Ф.4 (см. §6).
3. Конверсия ошибки при пробросе не должна терять cause (анти-Go `%w`-забыл; анти-Rust `From` без `source()`). → Ф.4.
4. Cleanup-ошибка не должна **молча перезаписывать** ошибку тела (анти-Go defer-overwrite) — только compose в suppressed. → Ф.2/Ф.4.
5. **Error return traces (Zig) — аналога не было в плане.** Zig в Debug/ReleaseSafe пишет адрес каждой точки пропагации и печатает трассу ПУТИ ошибки при uncaught (`@errorReturnTrace()`); стоимость только на error-path. Это debug-observability, не soundness. **Минимальная планка → Ф.5 п.7** (uncaught throw/panic в debug-билде печатает throw-site file:line; транспорт централизован — fail-frame + `nova_scope_exit` = дешёвая инструментация); **полный propagation-trace** (ring-buffer rethrow-точек) → `[M-173-error-return-trace]` (§9), якорь Ф.4-инфра.
6. **Стоимость happy-path пропагации (Swift):** Swift typed throws ≈ Result-под-капотом (swifterror-регистр, ноль unwind-таблиц). Nova longjmp-транспорт допустим (кадры ставят только handlers/defer), но инвариант обязан удержаться после Ф.2: **пропагация через `Fail[E]`-fn БЕЗ локального `with`/defer = НОЛЬ setjmp-кадров** → disasm-guard (§5 п.3).
7. **Мост эффект→значение:** Zig `catch`-выражение / Swift `Result(catching:)`/`try?` делают error→value тривиальным. Nova: value-сторона есть (`?`, `!!`, `??`); каноническая форма `Fail[E] → Result[T,E]` = идиома `with Fail[E] = |e| interrupt Err(e) { Ok(body) }` — **задокументировать в хабе (Ф.2)**; отдельный std-сахар в 173 НЕ вводится (кандидат на followup по спросу).

**Референс concurrency-API:** Java **JEP 533 `StructuredTaskScope`/`Joiner`** + Kotlin `coroutineScope`/`supervisorScope` + Swift `withThrowingTaskGroup` (implicit cancel + await-all).

---

## 3. Рекомендованный дизайн — MODEL 1 «defer — это ядро»

Единый примитив `defer` с опциональным outcome-биндингом; `Cleanup[E]` (ex-`Consumable`)/`consume`/`@cleanup`
(ex-`@on_exit`) низводятся до **сахара** над outcome-defer; **одна** точка ре-диспатча unwind.

**3.1. `defer` (без изменений формы)** — безусловный cleanup на ЛЮБОМ exit (normal/return/throw/panic/interrupt),
кроме `exit()`. LIFO. **Семантика добегания — §3a: completes-by-default** (щит элидится для sync-тел).

**3.2. `defer(o ScopeOutcome) { … }` (НОВОЕ — Idea B владельца)** — outcome-несущий block-defer; тело
получает `Success | Failure(any) | Panic(str)`. Субсумирует все три ретрактнутые формы:
- `errdefer{…}` ≡ `defer(o){ match o { Failure(_) | Panic(_) => … } }`
- `okdefer{…}` ≡ `defer(o){ match o { Success => … } }`
- `defer |result| {…}` (D189) ≡ эта форма с типизированным биндингом.
- **Zig-парность:** `errdefer |err| {…}` ≡ `defer(o){ match o { Failure(e) => use(e) } }` — payload `e`
  типизирован (богаче Zig-тега); ветка `Panic` различима И выполняется (Zig defer при panic не бежит).
Codegen тривиально на существующем defer-frame (`emit_c.rs:17613+`): setjmp уже знает исход,
`ScopeOutcome*` строится как в consume. ~~Bare `defer(o)` — unshielded по умолчанию~~ **⚠ ПЕРЕКРЫТО §3a:
completes-by-default, щит элидится для sync-тел.**

**3.3. `consume X = e { body }` → САХАР** над outcome-defer:
`{ ro X = e; defer(o) { X.@cleanup(o) }; body }`. `Cleanup[E]` остаётся как protocol-сахар (тип
инкапсулирует свой cleanup). Разница `consume` vs bare `defer(o)` — **must-consume-контракт (D133) +
exit-policy** (exactly-once инвариант D188 R2 + partial-init R1 + ResourceTrace-события), а НЕ
«добежит/не добежит» (§3a). Снимает хрупкий parser-lookahead `consume X = e {` vs
`consume X = e` (D180/D196 form-4 «partial»).

**3.4. Централизованный ре-диспатч (структурно чинит баг #1)** — ОДИН runtime-helper
`nova_scope_exit(frame, outcome_kind)`, вызываемый КАЖДЫМ setjmp-кадром (defer, consume-сахар, with-Fail):
PANIC → `nv_panic`; CANCEL → `nova_throw_cancel_reason`; USER/USER_TYPED → handler-recoverable (иначе
`nova_rethrow_with_suppressed`). Класс «один кадр забыл kind» исчезает по построению.

**3.5. Сохранение D194 hot-path:** сейчас `Consumable[Never]` + без `WithExitTimeout` элидит
shield/timeout/outcome (disasm-verified, T2.9). После лоуэринга элизия пере-ключается на признак
«**sync-тело + cleanup effect-row = `Fail[Never]`** → прямой вызов без кадра» (§3a-совместимая формулировка).
Acceptance: disasm Mutex/Sem/atomic ≡ до рефактора.

*(Историческое: fallback MODEL 2 рассматривался до sign-off 2026-06-20; Model 1 зафиксирована, Model 2 снята.)*

---

## 3a. Пересмотр cleanup/timeout (owner, 2026-06-26) — НОРМАТИВЕН, перекрывает §3.1/§3.2

> Ретрактирует D192 (force-таймаут на cleanup). Принцип владельца: **«defer всегда должен добегать»**.

**(1) `defer` ВСЕГДА добегает (completes-by-default); щит ЭЛИДИТСЯ для синхронных тел.**
Отмена в Nova **кооперативна** (доставляется только на suspend-точках). Поэтому:
- **синхронное** тело defer добегает и так → щит = no-op → компилятор его **элидит** → ноль стоимости;
- **suspend'ящееся** тело — cancel замаскирован на время cleanup → не рвётся посередине.
Bare `defer`/`defer(o)`/`consume` — ВСЕ completes-by-default. Модель **Go/Swift/Rust**, не Kotlin
(opt-in `NonCancellable` — footgun).

**(2) НЕТ force-таймаута, прерывающего cleanup — D192 ретракт.**
Никто из 7 языков не force-прерывает cleanup (вернуло бы partial-cleanup-порчу). «Инжект
`CleanupTimeoutError` в suspend cleanup'а» (D188 R3) **ретрактируется**. Зависший cleanup = баг
программиста; защищаемся снаружи (п.3), не разрывом изнутри. **Следствие для Ф.5:** дефект #10
закрывается УДАЛЕНИЕМ заглушки + watchdog-варн (порог — опционально через 3-level resolution), НЕ
достройкой force-механизма; `CleanupTimeoutError` как outcome cleanup'а исчезает — превышение фиксируется
в ResourceTrace exit-событии как duration/overrun-флаг.

**(3) Bounded-shutdown — дедлайн на SCOPE, не на cleanup.** Лимит времени — параметр `supervised`/`parallel`:
- **`supervised(deadline: Monotonic)`** — абсолютная точка (канон), пропагируется во вложенные scope;
- **`supervised(timeout: Duration)`** — относительный сахар = `deadline: Monotonic.now() + d`;
- типы — **Plan 175** (гейт: READY, не начат — prerequisite-check §7a); параметр рядом с `cancel:`
  *(набор параметров: `cancel:`/`deadline:`/`timeout:` — БЕЗ `strategy:`/`max_restarts:`/`period:`,
  которые отменены пересмотром 173.2/§3b)*;
- **`parallel for` зеркалит** параметры `supervised` и/или наследует дедлайн enclosing-scope.
По дедлайну → кооперативная отмена детей → cleanup'ы добегают. Превышение самого дедлайна при зависшем
cleanup → **watchdog-варн** («fiber X застрял в cleanup»), НЕ force-kill.

**(4) `with_timeout` — убрать; `race2` — оставить до общего `race`.**
- **`with_timeout[T]`** (`std/concurrency/cancellation.nv:156`) — субсумирован `supervised(timeout:)`.
  Удалить (fn + ~7 тест-ссылок) **после** landing Plan 175 + deadline-параметров → **Ф.3-остаток**.
- **`race`/`race2`** — ⚠ Ред. 2 согласовано с [173.1 §2a](173.1-parallel-collect-and-supervised-value.md)
  (авторитет): `race2` **ОСТАЁТСЯ** до landing общего N-арного `race[T](…funcs)` (гейт Plan 48 Ф.4
  closures-in-generic-array) + миграции callers; дизайн общего `race` уже зафиксирован в 173.1 §2a
  (канал сериализует победителя). «Убрать сейчас» из ранней редакции НЕ действует.
- Спека stale: 06-concurrency.md «race/with_timeout — stdlib поверх примитивов» + `race { … }`-блок
  (04-effects.md:94, не реализован) — поправить (Ф.5 sweep).

**(5) Per-op timeout — fallback** (`ch.recv(timeout:)` и т.п.). Не замена scope-дедлайна.

**Амендить при реализации:** D188 R3 (completes-by-default + elision), **D192 РЕТРАКТ**, D71/`parallel for`
+ `supervised` (deadline:/timeout:), 06-concurrency.md / 04-effects.md (stale race/with_timeout).
Зависит от **Plan 175**. Координация: 173.0 (cancel/shield-mask), 173.1 (parallel/supervised value + параметры).

---

## 3b. Пересмотр supervisor-модели: restart для файберов (owner, 2026-06-26) — НОРМАТИВЕН

> Перекрывает Ф.0-решение «все 3 стратегии OneForOne/OneForAll/RestForOne + max_restarts/period».
> **Erlang-restart завязан на process-ИЗОЛЯЦИЮ, которой у файберов нет.**

**Проблема.** В Erlang дети — процессы (share-nothing): restart = чистый сброс. У Nova дети — файберы на
**общей памяти**: «restart файбера» переиспользует испорченное общее состояние → гарантия «вернулись в
известно-хорошее» НЕ держится. Ни один файбер/корутин-фреймворк (Kotlin, Java StructuredTaskScope, Swift
TaskGroup, Go errgroup) restart НЕ берёт.

**Решение (owner 2026-06-26):**
- **(a) default — БЕЗ per-fiber restart:** `cancel`-siblings / **Escalate** / **Stop** / aggregate.
- **(b) restart — ТОЛЬКО для ИЗОЛИРОВАННЫХ файберов, opt-in:** рычаг изоляции —
  [173.3](173.3-data-race-freedom-share.md) `#share`/consume-в-spawn.
- **(c) опционально — restart на уровне ВСЕГО scope** (retry операции целиком), не отдельного файбера.

**Резолюция для 173.2 (Ред. 2, внести в 173.2 на Ф.0R):** 173.2-MVP заявляет `Restart(single)` с гардом
«запрет consume/move-захватов» (173.2 п.7) — этот гард **слабее** §3b(b) (не запрещает shared-mut захваты).
Амендмент: **MVP 173.2 = Escalate/Stop (+cancel)**; `Decision.Restart` остаётся в словаре `Decision`, но
его ИСПОЛНЕНИЕ гейтится изоляцией — минимум: расширить гард 173.2 п.7 до «тело spawn не захватывает
shared-mut» (компилируемое приближение); полный рычаг = 173.3 `#share`. Superseded-помету — в 173.2.

**Затрагивает:** 08-runtime.md:168 (`OneForOne`/`max_restarts` заявлены — stale, поправить в Ф.5 sweep);
[173.1:77](173.1-parallel-collect-and-supervised-value.md) (параметр-сет содержит `strategy:/max_restarts:/period:`
— убрать на Ф.0R); координация 173.0/173.2.

---

## 4. Фазы

> **Порядок исполнения:** Ф.0 ✅ → **Ф.0R → Ф.1 → Ф.2** (этот цикл, строго последовательно) →
> **Ф.3 = семейство [173.0](173.0-concurrency-runtime-substrate.md) → [173.1](173.1-parallel-collect-and-supervised-value.md)
> → [173.2](173.2-supervision-as-effect.md) → [173.3](173.3-data-race-freedom-share.md)** (авторитетны; зонт держит
> Ф.3-остаток) → **Ф.4** (гейт: Plan 174.3) → **Ф.6**. **Ф.5 — СКВОЗНАЯ** (гигиена per-phase; заголовочные
> пункты закрываются по мере готовности зависимостей). «Позже» = обязательно, другим циклом; не урезается.
> **Prerequisite-протокол:** при старте фазы проверить гейты (§7a п.3); незакрыт → СТОП, эскалация владельцу.

### Ф.0 — Дизайн + sign-off владельца ✅ ЗАКРЫТА (2026-06-20; пересмотры §3a/§3b внесены Ред. 2)
Model 1 зафиксирована; синтаксис `defer(o ScopeOutcome)`; D-блок — **D314** (verify Ф.0R). Все решения — §6.

### Ф.0R — Семейная сверка (СЕЙЧАС, перед Ф.1; small)
Механическая синхронизация после пересмотров §3a/§3b (тексты правок готовы — просто внести):
1. **[173.1:77](173.1-parallel-collect-and-supervised-value.md)**: из параметр-сета `supervised`/`parallel`
   убрать `strategy:/max_restarts:/period:` (оставить `deadline:/timeout:/cancel:`) — эхо отменённого решения.
2. **[173.2](173.2-supervision-as-effect.md)**: superseded-врезка per §3b-резолюция (MVP = Escalate/Stop;
   `Restart(single)` за гейтом изоляции; расширить гард п.7 до shared-mut).
3. **D-нумерация verify:** D314 зарезервирован за 173 (`172.1-d-status.md:411`) — подтвердить свободу;
   Ф.6 D-блок = **D348** (D340-D346 serde, D347 Plan 181; D348+ свободны — проверено 2026-07-03). NB
   попутная находка: Plan 178 претендует на D327-D332, а D327 уже занят в спеке — коллизия НЕ наша,
   передать владельцу Plan 178 (заметка в discussion-log).
4. **Маркеры в OPEN-view:** добавить строки `[M-172-with-fail-swallows-panic]`, `[M-172-errdefer-okdefer-dead-surface]`
   (home = 173 Ф.1) и `[M-173-error-return-trace]` в `docs/plans/backlog-followups.md` (детали маркеров
   М-172 живут в `docs/backlog-followups.md:169,185` — там обновить stale file:line на актуальные из §1).
5. **Хаб:** точечный статус-фикс (баннер «§3a/§3b 2026-06-26: completes-by-default, D192-ретракт,
   no-restart-default») — полный rewrite будет в Ф.2.
- **Acceptance:** 173.1/173.2 не содержат отменённых knobs; D314/D348 подтверждены; маркеры в OPEN-view;
  **без упрощений**.

### Ф.1 — Soundness + hygiene (СЕЙЧАС, обязательно; design-risk ноль)
Багфиксы, не зависящие от деталей Model 1:
1. **#1 fix:** `with Fail` ре-throw'ит PANIC (ветка перед CANCEL/USER, `emit_c.rs:6885+`); сразу ввести
   общий helper `nova_scope_exit` (предшественник Ф.2 п.4). Закрывает `[M-172-with-fail-swallows-panic]`.
2. **#3 fix:** удалить `?` Fail-context throw-mode (`emit_c.rs:21895-21958`) → `[E_TRY_IN_FAIL_FN]`;
   `?` строго return-only. Завершает codegen-часть [174.2](174.2-question-mark-return-only.md).
3. **#2 fix:** диагностика D133 — quick-fix на `defer`/`@cleanup` вместо errdefer/okdefer
   (`types/mod.rs:18764+, 18819+`, D162-quickfix `:19636+`).
4. **#4 fix:** удалить мёртвый errdefer/okdefer/defer|result| surface (AST/lexer/DeferKind/codegen-ветки —
   полный список в §1 #4); оставить лишь tombstone-распознавание для D189-hint. Закрывает
   `[M-172-errdefer-okdefer-dead-surface]`.
5. **interim-guard #7-concurrency (P1):** чекер отвергает неподдержанный результат `parallel for`
   (элемент ∉ {int,bool,f64,str} при value-позиции) чистым `[E_PARFOR_RESULT_UNSUPPORTED]` — вместо
   молчаливого degrade и сырого C-error. Снимается фиксом 173.1 Ф.2 (guard остаётся как unreachable-защита).
- **spec/docs:** `## D4` + дубль (:950) → RETRACTED-баннер; `spec/decisions/README.md` строки 18/19/36 fix;
  хаб — статус-фикс (Ф.0R п.5 если не сделан).
- **Тесты** (раскладка — §4a): pos `rt/f1_with_fail_swallow_panic.nv` (panic сквозь `with Fail` → процесс
  падает с `panic:`); pos `f1_try_return_only` (`?` на Result/Option). neg: `?` в Fail-fn →
  `[E_TRY_IN_FAIL_FN]`; `errdefer{}`/`okdefer{}` → `[D189-removed-*]`; непримитивный `parallel for`-результат
  → `[E_PARFOR_RESULT_UNSUPPORTED]` (не сырой C-error). spec_tests/conformance: d85/d13-покрытие затронутого.
- **Acceptance:** #1/#2/#3/#4 + interim-guard закрыты; spec_tests зелёный; nova_tests baseline-delta = 0;
  disasm hot-path не деградировал (baseline = parent-коммит Ф.1, процедура §7 п.9); **без упрощений**.

### Ф.2 — Унификация defer-kernel (СЕЙЧАС после Ф.1)
1. parser+AST: `defer(o ScopeOutcome) { … }` (биндинг + тело).
2. codegen: outcome-defer на defer-frame; материализация `ScopeOutcome*` и в success-ветке.
3. `consume`/`@cleanup` → desugar в `defer(o) { X.@cleanup(o) }` (щит имплицитен — §3a); `Cleanup[E]` =
   protocol-сахар.
4. **Структурный финал бага #1 (реализация §3.4):** весь ре-диспатч через единый `nova_scope_exit`.
5. D194-элизия пере-ключена: «**sync-тело + cleanup `Fail[Never]`** → прямой вызов без кадра»; disasm-парность.
6. **Multi-binding `consume`** (подтверждено 2026-06-20): `consume a = e1, b = e2, c, (x,y) = e3 { body }`
   ≡ вложенные consume-блоки (LIFO + partial-init D188 R1 бесплатно из вложенности; закрывает
   Zig errdefer-in-loop footgun). Десугар: каждый биндинг → `ro X = e; defer(o) { X.@cleanup(o) }`.
7. **Rename (sign-off 2026-06-20; ДОМ — Ф.2, не Ф.5):** протокол `Consumable[E]` → **`Cleanup[E]`**,
   метод `@on_exit` → **`@cleanup(o ScopeOutcome)`** — везде (`std/prelude/protocols.nv`, codegen,
   consume-тесты). Amend D188/D194.
8. **Rename эффекта (освобождает имя ДО п.7):** D185 `Cleanup` effect → **`ResourceTrace`**; операции →
   `on_resource_enter(label)` / `on_resource_exit(label, outcome)` (per-resource, LIFO); `timeout` из
   enter убран. Затронуть parser+codegen-dispatch, 3 теста `plan110/cleanup_*`. Amend D185.
9. **Хаб — ПОЛНЫЙ REWRITE** (единственный; Ф.5 лишь верифицирует). Чеклист: D314-ядро; defer(o);
   consume=сахар; renames п.7/8; `nova_scope_exit`; completes-by-default (§3a); no-restart-default (§3b);
   catchability-инвариант (Ф.4 п.6); идиома `Fail[E]→Result` (§2 риск 7); новая миграционная таблица
   (errdefer→`defer(o){match Failure}`); НЕ-цели (call-site try-маркер — видимость на уровне сигнатуры;
   HOF-эффект-полиморфизм аналог Swift rethrows — вне периметра); SPDX `<!-- CC-BY-4.0 -->` (сейчас
   стоит `// MIT OR Apache-2.0` — docs лицензируются CC-BY-4.0, AGENTS.md:133); минимум file:line.
10. **Doc-sweep rename:** grep `Consumable|on_exit|errdefer` по docs/ вне plans/ (~24 файла:
    cleanup-cookbook, tutorial-cleanup, idiom/*, nv-coding-style §20.4, nova-cli.md/.ru) — обновить.
- **spec/D/Q/docs:** **D314** (defer-kernel, spec-first: написать D-блок ДО codegen) + amend D188 (consume
  = сахар; R3 → completes-by-default) / D90 (defer-family) / D189 (формы возвращены как `defer(o)`) /
  D185 / D194; Q-cleanup-semantics обновить.
- **Тесты** (раскладка §4a): pos `f2_defer_outcome_*` — Success/Failure/Panic ветки; **errdefer-эквивалент
  С ЗАХВАТОМ payload** (`Failure(e) => use(e)` — Zig-парность, payload богаче тега); **`Panic(str)`-ветка
  различима И выполняется при panic** (Zig/Swift-превосходство под тестом); okdefer-эквивалент;
  consume-as-sugar (тот же результат, что старый consume); panic-in-defer-body composition; LIFO с outcome;
  multi-binding LIFO; partial-init (e2 бросил → чистится только a; помета «закрывает Zig errdefer-in-loop»);
  bare-c adopt; tuple consume. neg: `defer(o)` с top-level return/break/continue/interrupt в теле (D90);
  двойной биндинг. spec_tests/conformance: `d314_*.nv` (ядро), `d188_*`, `d189_*`, `d90_*` (амендменты).
- **Acceptance:** старые consume/on_exit-тесты зелёные через сахар; bare `defer(o)` работает; единый
  re-dispatch (нет per-frame дублирования); renames применены везде (grep `Consumable|@on_exit` по
  std/+docs/ = 0 вне historical); хаб переписан; disasm hot-path ≡; **без упрощений**.

### Ф.3 — Structured-concurrency error handling (ПОЗЖЕ, обязательно)

> **⚠ ВЫНЕСЕНО В СЕМЕЙСТВО 173.0-173.3 (sign-off 2026-06-21) — под-планы АВТОРИТЕТНЫ:**
> **[173.0](173.0-concurrency-runtime-substrate.md)** (рантайм-субстрат, ГЕЙТ: per-slot ошибки + serialized
> decision-loop + ctx-pinning + drain-гонка) → **[173.1](173.1-parallel-collect-and-supervised-value.md)**
> (`parallel for → []T` канал+consume, completion-order dense, `supervised`-значение; закрывает
> `[M-parfor-record-result-miscompile]`) → **[173.2](173.2-supervision-as-effect.md)** (supervision =
> эффект `Supervisor`/`on_child_fail(idx,err,attempt)→Decision`; с Ф.0R-амендментом: MVP Escalate/Stop,
> Restart за гейтом изоляции) → **[173.3](173.3-data-race-freedom-share.md)** (`#share`, capture-check,
> consume-в-spawn).
> Ранние пункты про `[]Result`/`all_or_throw`-номенклатуру и sum-type `SupervisorStrategy` — SUPERSEDED
> (scope-result = канал + completion-order dense; стратегии = эффект-хендлеры).

**Ф.3-остаток (живёт В ЭТОМ плане — НЕ покрыт под-планами; закрыть до объявления Ф.3 done):**
1. **PANIC vs CANCEL vs USER precedence-политика** при нескольких retained-ошибках разных kind:
   какой становится primary при default-Escalate (D13-инвариант: PANIC не деградирует до ловимого USER;
   правило: PANIC > USER > CANCEL при выборе primary; остальные — в suppressed). Стыкуется с Ф.4 п.6.
   (`fibers.h:1763` USER-beats-CANCEL CAS — заменяется 173.0 retention + этой политикой.)
2. **detach error-policy + enforce `Detach`-эффекта** (сейчас unenforced, 06-concurrency.md:919):
   policy-словарь (LogAndDrop default / escalate-to-scope opt-in), enforcement в checker.
3. **channel closed-vs-value:** решение Ред. 2 — `recv → Option` ОСТАЁТСЯ (канон); верифицировать
   `None = rx.recv()`-арм в select (D94) + добавить тест различения + doc-пример. Согласовано с
   173.1-десугаром (стоит на Option).
4. **stale-тесты** `supervised_errors.nv:213`, `fiber_throw.nv:110` («throw неперехватываем» — ложь) —
   переписать на актуальную семантику.
5. **`with_timeout` удаление** (§3a п.4) — после Plan 175 + deadline-параметров.
- **spec/D/Q/docs:** D-блок «structured error propagation» (номер — следующий свободный ПОСЛЕ D348:
  **D348 зарезервирован за Ф.6**, т.е. D349+ по high-water на момент реализации); amend 06-concurrency.md
  (D14/D75/D50/D94; 08-runtime.md:168 stale-стратегии); docs/idiom/*.
- **Тесты:** по под-планам + остаток: pos «3 ребёнка падают → primary + 2 в suppressed (НЕ first-wins-only;
  строго лучше Swift TaskGroup)» (после Ф.4 — MultiError-форма); precedence PANIC>USER>CANCEL; detach-policy;
  select None-arm различает closed. neg: `[E_PARFOR_RESULT_UNSUPPORTED]` до 173.1-фикса; detach без
  Detach-эффекта → E-код.
- **Acceptance:** child-fail отменяет siblings + retention by default; после Ф.4 — агрегация MultiError
  (≡ Kotlin/Java, лучше Swift); detach enforced; ни одна ошибка fiber'а не теряется молча;
  `parallel for → []T` для любого T (173.1); **без упрощений**.

### Ф.4 — MultiError end-to-end + типизированный ScopeOutcome (ПОЗЖЕ, обязательно; 🔴 ГЕЙТ: [Plan 174.3](174.3-any-type-and-is-downcast.md))
1. **#6:** материализовать `NovaErrorChain` → Nova `MultiError` в точке получения composed-ошибки
   (handler-arm/scope-result); использовать ГОТОВЫЕ read-аксессоры `nova_failframe_suppressed_count/at`
   (`effects.h:269-283`); методы = уже объявленные в `errors.nv:207-250`.
2. **#5:** `ScopeOutcome.Failure(any)` — протянуть `error_user_payload`/`type_id`; типизированный
   `if e is T` в `@cleanup`; `core.nv:147` → `Failure(any)`.
3. **#7:** инвариант suppressed-chain: убрать безусловный `error_suppressed=NULL` (`effects.h:93,114,131,801`,
   NOVA_TRY :285) ИЛИ маршрутизировать все cleanup-throw через `nova_rethrow_with_suppressed`.
4. typed-предикатный доступ: **`e is T`** (D54-семантика, инфра 174.3); `.downcast[T]()` НЕ вводится (§6).
5. *(surface-вопрос закрыт — §6: имена = `errors.nv`, typed-доступ = `is`.)*
6. **Инвариант catchability (две ошибки → MultiError НЕ подменяет эффект):** primary остаётся носителем —
   `nova_rethrow_with_suppressed` (`effects.h:210-218`) копирует `type_id+payload` примери; `with Fail[Primary]`
   СРАБАТЫВАЕТ, эффект НЕ становится `Fail[MultiError]`; `e.suppressed()` = [cleanup]. Инвариант обязан
   пережить Ф.2-унификацию — regression-guard. (Упала ТОЛЬКО cleanup → primary = cleanup-ошибка.)

**Фундамент typed-errors — type_id-инфра Plan 61 (готова):** compile-time `NOVA_TID_<E>`
(`type_id_registry`, `emit_c.rs:1241`), typed-throw несёт `(payload, tid)`, матчинг в arm'е handler'а
(`fail_e_map`, `emit_c.rs:1249`) → `is T` = та же проверка `type_id == NOVA_TID_T`. `any`-boxing/vtable —
[Plan 174.3](174.3-any-type-and-is-downcast.md) (📋 PROPOSED, не начат — **реализуй ПЕРВЫМ**; Ф.4 полностью
заблокирована до него). Ф.4 строит на готовом фундаменте, не с нуля.
- **spec/D/Q/docs:** D158/D193 завершить (materialization); D190 (`ScopeOutcome[E]` остаётся rejected); хаб-верификация.
- **Тесты** (раскладка §4a): pos: primary+suppressed видны в handler; typed Failure dispatch; cleanup-fail
  во время body-fail → MultiError (не overwrite); chain переживает голый throw в unwind; **catchability**:
  тело бросает `Primary`, `@cleanup` бросает `Cleanup2` → `with Fail[Primary]` ловит (`e` = Primary,
  type_id сохранён), `e.suppressed() == [Cleanup2]`. neg: упала ТОЛЬКО cleanup (`Cleanup2` ≠ `Primary`) →
  `with Fail[Primary]` НЕ ловит; **неexhaustive match по вариантам E в handler-arm → compile-error**
  (парность Zig exhaustive error-set switch / Swift typed catch). spec_tests: d158/d193-финализация.
- **Acceptance:** D158/D193 выполнено end-to-end; cleanup-ошибка НИКОГДА не перезаписывает body-ошибку;
  catchability-инвариант под regression-guard; **без упрощений**.

### Ф.5 — Hygiene: exactly-once + watchdog + traces + spec-sweep (СКВОЗНАЯ: пункты закрываются по готовности зависимостей)
1. **#8:** D188 R2 exactly-once runtime-счётчик `_consume_count` + `D188-on-exit-double-invocation`
   (production-grade защита от ручного/FFI double-invoke) — НЕ сводить к структурному (упрощение).
2. **#10 (в §3a-редакции):** УДАЛИТЬ force-timeout заглушку (`effects.h:256-260`); вместо неё —
   **watchdog-варн** «fiber застрял в cleanup» при превышении порога (порог: 3-level resolution
   `WithExitTimeout` vtable → Application → default — сохранить как источник ПОРОГА варна, не прерывания);
   превышение наблюдаемо в ResourceTrace exit-событии (duration/overrun-флаг). `CleanupTimeoutError` как
   outcome НЕ существует (D192-ретракт).
3. **#9:** `nv_resume_panic` → `nv_panic` в D188/D197 спек-тексте (`03-syntax.md:8161`) — или ввести
   реальный primitive (решение: спек-фикс, primitive не нужен).
4. **#11 sweep:** D90 §errdefer / **D158 (errdefer-канон в тексте; materialization-часть — Ф.4)** /
   D160-тело / D161 → historical с баннером «see D314/D188/D189»;
   D162-таблица; 08-runtime.md:168 (stale OneForOne); 06-concurrency.md/04-effects.md stale race/with_timeout
   (§3a п.4); README-индекс. (D4/README-строки — уже сделано в Ф.1; здесь верификация.) Хаб — верификация
   против Ф.2-rewrite (НЕ второй rewrite).
5. *(renames — ВЫПОЛНЕНЫ в Ф.2 п.7/8; здесь — только spec-amend верификация D185/D188/D194 текстов.)*
6. **`nova_runtime_reset()`** между panic-тестами в одном процессе — инфра для Ф.6 (re-entry hazard:
   висящий `_nova_fail_top`/handler-iframe между N паниками).
7. **Error-trace минимум (Zig-парность, §2 риск 5):** uncaught throw/panic в debug-билде печатает
   throw-site (`file:line`) — инструментация в fail-frame/`nova_scope_exit`. Полный propagation-trace →
   `[M-173-error-return-trace]`.
- **Тесты** (раскладка §4a): pos: double `@cleanup`-invoke → `D188-on-exit-double-invocation`; watchdog-варн
  наблюдаем (превышение порога → stderr-варн, cleanup ДОБЕГАЕТ); debug uncaught-trace печатает file:line.
  neg: D192-ретракт — ссылка на `CleanupTimeoutError` / упоминание force-timeout API → compile-error
  (тип/символ удалён); `nova_runtime_reset` вне test-frame недоступен из user-кода.
  spec_tests: d188 R2 + d192-ретракт (негативная сторона).
- **Acceptance:** спека выводит ТЕКУЩУЮ модель без реверс-инжиниринга кода; exactly-once реален; force-timeout
  механизм ОТСУТСТВУЕТ (D192-ретракт применён); **без упрощений**.

### Ф.6 — `panics`-клаузула: panic-тесты в folder-module (−78 CU; ПОЗЖЕ)
Контекстное KW `panics` (инверсия PASS/FAIL): `test "…" panics "паттерн" { … }` — PASS, если тело
запаниковало сообщением ⊇ паттерн (substring, как D89). Складывает ~114 runtime-panic тестов (36 папок)
в folder-module → **−78 CU** (цель [169.1.2](169.1.2-consolidate-tests.md)).
**Гейт:** Ф.1 (panic не глотается) + Ф.5 п.6 (`nova_runtime_reset`) — по имени, не по номеру пункта.
- **spec-first — новый D-блок D348** (09-tooling.md): семантика panics-клаузулы (инверсия, substring,
  exit=0, granularity per-test) + **amend D89** (шестой маркер-класс; инверсия PASS/FAIL) + правка
  таблицы folder-module layout 09-tooling.md:2938-2943 (строка EXPECT_RUNTIME_PANIC/rt/ → «legacy; новые
  runtime-panic = panics-клаузула»).
- parser+AST: `TestDecl { …, panics: Option<String> }` (контекстное KW, как `raw`/`bench`).
- codegen (test-frame setjmp, `emit_c.rs:17218+`): при `panics.is_some()` инвертировать ветки +
  `strstr(msg, pattern)`; exit=0 при успехе; между тестами — `nova_runtime_reset()`.
- миграция: `fn main` + `// EXPECT_RUNTIME_PANIC <pat>` → `test "<stem>" panics "<pat>"`.
  `EXPECT_RUNTIME_PANIC` остаётся для legacy + селектора `--panic` ([169.1.1](169.1.1-test-lane-flags-and-ci.md)).
- граница: ТОЛЬКО runtime-panic (НЕ compile-error/timeout/exit — остаются `fn main`/`neg/`).
- **Правки test-conventions.md (по governance: каждая с «· согласовано»; sign-off Ф.6 владельцем =
  согласование):** (a) секция «EXPECT_RUNTIME_PANIC и fn main()» (:429-443) + таблица «куда класть»
  (:850-860) + naming (:353) → новая норма panics-клаузулы; (b) починить stale-ссылку :295-296
  «panics-клаузула (Plan 169.1.2 Ф.2)» → Plan 173 Ф.6; (c) **попутная дыра конвенций** (обозначена
  аудитом 2026-07-03, правится здесь же по разрешению владельца менять конвенции): унифицировать список
  маркеров (заголовок «5 стандартных» vs «4» vs фактические 7+ — EXPECT_TIMEOUT/EXPECT_LINT_WARNING/
  EXPECT_TIMEOUT_MS) + убрать порог «run > 2s → _slow» (:453) в пользу единственной точки правды D298.
- **Тесты:** pos (.nv): ожидаемая паника → PASS; **N паник в одной folder-module не ломают рантайм**
  (Ф.5-reset). **Мета-FAIL-кейсы («неверный паттерн → FAIL», «нет паники → FAIL») — НЕ .nv-фикстуры**
  (сделали бы suite красным навсегда): Rust-интеграционные тесты раннера в `compiler-codegen/tests/`
  поверх test_runner.rs. spec_tests: d348_*.nv (семантика клаузулы — позитивная сторона).
- **Acceptance:** 114 panic-тестов в folder-module зелёные; −78 CU; рантайм стабилен после N паник;
  D348 в спеке; test-conventions обновлены с «· согласовано»; **без упрощений**. Маркер: `[M-173-panics-clause]`.

---

## 4a. Раскладка тестов (конвенция — test-conventions.md, выверено 2026-07-03)

**ОДНА папка `nova_tests/err173/`** (folder-module `module nova_tests.err173`) — НЕ шесть
(test-conventions: максимизируй folder-module, минимизируй CU; фаза — в ИМЯ файла):
- позитивы — peer-файлы `f1_*.nv … f6_*.nv` (фазовый префикс сохраняет навигацию);
- compile-error — `neg/<name>.nv` (`module neg.<name>`, `EXPECT_COMPILE_ERROR`);
- runtime-panic ДО Ф.6 — `rt/<name>_panic.nv` standalone (`fn main` + `EXPECT_RUNTIME_PANIC`);
  ПОСЛЕ Ф.6 — мигрируют в panics-клаузулу peer-файлов.

**spec_tests/conformance — ОБЯЗАТЕЛЬНОЕ D-покрытие** (методология 2026-06-28: на каждый затронутый
D-блок — `d<NNN>_<кратко>.nv`; один CU): D314 (ядро), D348 (panics), амендменты D188/D189/D90/D185,
финализация D158/D193, ретракты D4/D192 (негативная сторона — где применимо); **Ф.3:**
`d<NNN>_structured_propagation.nv` (номер по high-water, D349+) + амендменты D14/D75/D50/D94.

**Прогон:** targeted `nova test nova_tests/err173` + `nova test spec_tests` per-task; полный регресс —
батчами (§7 п.8) на фазовом закрытии. `nova test` требует явный путь (fd7a8da5).

---

## 5. Сквозные критерии приёмки

1. **«Без упрощений, как для прода»** (ОБЯЗАТЕЛЬНЫЙ) — никаких заглушек/TODO в закрываемой функциональности;
   каждый дефект §1 закрыт реально (не задокументирован-как-известный).
2. **spec_tests/conformance зелёный** + **nova_tests baseline-delta = 0** после каждой фазы (baseline =
   тот же коммит-родитель через temp-worktree/commit+reset — §7 п.1; nova_tests НЕ чистый гейт корректности,
   годен только как delta); новые тесты по раскладке §4a.
3. **Disasm hot-path** (`Mutex`/`Semaphore`/atomic, `Cleanup[Never]`) не деградировал (процедура — §7 п.9)
   **+ frame-free propagation:** вызов `Fail[E]`-fn из fn с тем же эффектом без локального handler/defer →
   0 setjmp-кадров (стоимостная парность Swift typed-throws ABI).
4. Планка «не хуже Go/Rust/TS/Kotlin/Java/Zig/Swift» (§2): все 7 «рисков» закрыты, все «выигрыши»
   сохранены (regression-guard тесты; в т.ч. Zig-строки: payload-errdefer, Panic-ветка бежит, partial-init).
5. Спека/D/Q/docs синхронны с кодом; хаб описывает ЕДИНУЮ модель (один rewrite в Ф.2); нет
   stale-противоречий (D4/errdefer/D192/OneForOne).
6. `panic` неперехватываем `with Fail`/`?`/handler'ом (D13), но ЗАПУСКАЕТ cleanup — оба инварианта под
   тестами. **(vs Zig/Swift: там panic/fatalError cleanup НЕ запускает — инвариант Nova строго сильнее.)**
7. Каждая фаза — отдельный коммит (или серия per-task); sync в main после фазы.

---

## 6. Вопросы Ф.0 — решения (ЗАКРЫТЫ; Ред. 2 внесла пересмотры §3a/§3b и дозакрыла остатки)

| Вопрос | Решение |
|---|---|
| Унификация: одна модель или две | **Model 1 «defer — ядро»** ✅ (sign-off 2026-06-20; Model 2 снята). |
| Idea A (`on_exit ⇒ defer`) vs Idea B (`defer(outcome)`) | **Idea B как основа**; Idea A — следствие (@cleanup = сахар). |
| shield/timeout default для `defer(o)` | ~~bare unshielded~~ **⚠ ПЕРЕКРЫТО §3a (2026-06-26): ВСЁ completes-by-default; щит элидится для sync-тел.** Разница consume vs bare = must-consume + exit-policy. |
| D194 hot-path | сохранить; критерий элизии = «sync-тело + cleanup `Fail[Never]`» (§3a-формулировка); disasm-парность — приёмка. |
| `?` + auto-`From` | **отклонён** ([174.2](174.2-question-mark-return-only.md)); explicit `.map_err`. |
| `ScopeOutcome.Failure` тип | **`Failure(any)`** (type-erased, D188); `ScopeOutcome[E]` rejected (D190). |
| MultiError | **материализовать end-to-end** (Ф.4). Catchability-инвариант: primary остаётся носителем; ловишь `Fail[Primary]`, cleanup — в `e.suppressed()`. |
| exactly-once (D188 R2) | **runtime-счётчик** (Ф.5 п.1), не структурное сведение. |
| exit-timeout | ~~реализовать 3 уровня~~ **⚠ ПЕРЕКРЫТО §3a (D192-ретракт): force-timeout НЕ существует; watchdog-варн, 3-level — только источник порога** (Ф.5 п.2). |
| concurrency: child-fail default | **Escalate + per-slot retention (173.0) сразу; проброс primary+suppressed→MultiError — после Ф.4** (до Ф.4 default = байт-в-байт нынешний all-or-throw с retention; порядок зафиксирован — инвариант §2 п.2 выполняется c Ф.4). |
| supervisor-стратегии | ~~все 3 OTP-стратегии sum-type-аргументом~~ **⚠ ДВАЖДЫ ПЕРЕСМОТРЕНО: (2026-06-21) стратегии = эффект-хендлеры [173.2](173.2-supervision-as-effect.md), без knob {max_restarts, period}; (2026-06-26, §3b) restart по умолчанию НЕТ (cancel/Escalate/Stop), Restart — за гейтом изоляции (минимум shared-mut-гард, полный — 173.3).** |
| detach | явная error-policy + enforce `Detach` → **Ф.3-остаток п.2**. |
| channel recv | **Ред. 2 ЗАКРЫТ: `recv → Option` остаётся каноном** (D91; 173.1 стоит на нём); дефект = select-wildcard семантика → верификация+тест (Ф.3-остаток п.3). Result-миграции НЕ будет. |
| PANIC/USER/CANCEL precedence | **Ред. 2 ЗАКРЫТ: primary выбирается PANIC > USER > CANCEL** (D13: PANIC не деградирует); остальные retained → suppressed (Ф.3-остаток п.1). |
| Multi-binding `consume` | ✅ подтверждено 2026-06-20: `consume a = e1, b = e2, c, (x,y) = e3 { body }` ≡ вложенные consume-блоки — **дом: Ф.2 п.6**. |
| Нейминг: cleanup-протокол | ✅ `Consumable[E]` → **`Cleanup[E]`**, `@on_exit` → **`@cleanup(o)`** — **дом: Ф.2 п.7**. |
| Нейминг: observability-эффект | ✅ D185 `Cleanup` → **`ResourceTrace`**, `on_resource_enter(label)`/`on_resource_exit(label, outcome)`, per-resource, timeout из enter убран — **дом: Ф.2 п.8** (освобождает имя до п.7). |
| Surface-синтаксис concurrency + MultiError | **Ред. 2 ЗАКРЫТ:** scope-result = **[173.1](173.1-parallel-collect-and-supervised-value.md)** (канал + completion-order dense; `supervised` возвращает trailing-значение) — номенклатура `all_or_throw`/`any_ok` SUPERSEDED; имена MultiError-аксессоров **уже зафиксированы кодом** `std/prelude/errors.nv:207-250` (`@primary/@suppressed/@walk/@find_first_panic`); typed-доступ = **`e is T`** (D54/174.3), `.downcast[T]()` НЕ вводится; opt-in `[]Result[T,E]`-форма при deadline/partial — единственный остаток, резолв одной строкой в 173.1 Ф.0. |
| Мост `Fail[E] → Result[T,E]` | **Ред. 2 ЗАКРЫТ:** идиома `with Fail[E] = \|e\| interrupt Err(e) { Ok(body) }` — документируется в хабе (Ф.2 п.9); std-сахар в 173 не вводится. |

**§6a. Делегированные Q семейства** (открыты в под-планах — там и резолвятся; зонт не дублирует):
[173.1](173.1-parallel-collect-and-supervised-value.md) — форма Channel-API (`new(cap)`/bounded/unbounded),
размер K (бенч), refcount-vs-close, `[]Result`-форма; [173.2](173.2-supervision-as-effect.md) — handler
effect-set (Fail=Escalate-with-error; suspend в хендлере), `Supervisor[E]` типизация err, `Decision.Use(value)`;
[173.3](173.3-data-race-freedom-share.md) §7 — 9 под-вопросов (#share-обязательства, poison-база `*T`-cell, …).

---

## 7. Исполнение фоновыми агентами (ОБЯЗАТЕЛЬНО соблюдать)

1. **НИКАКОГО `git stash`** — `.git` repo-global, конкурентные worktree → stash/refs глобальны
   (collision/потеря). Baseline — **temp-worktree** ИЛИ **commit+reset** в своей ветке, ИЛИ patch+checkout
   (`git diff > /tmp/wip.patch; git checkout -- .; … ; git apply /tmp/wip.patch`). НЕ stash.
   Канонический baseline-рецепт: `git worktree add ../nova-173-base <parent-commit>` → собрать там
   release-бинарь → прогнать → `git worktree remove ../nova-173-base`.
2. **Worktree:** постоянный воркер — `git worktree add -b plan-173 ../nova-p173 main` (naming nova-pNN);
   параллельные агенты — каждый в своём worktree (`isolation: 'worktree'`) ИЛИ непересекающиеся файлы.
   При копировании worktree: libuv-submodule скопировать + удалить его `.git`; env
   `NOVA_GC_INCLUDE_DIR`/`NOVA_GC_LIB_DIR` — на main-репо. После checkout в worktree — mtime-touch `.rs`
   (`find compiler-codegen/src -name '*.rs' -exec touch {} +`) перед cargo build (стейл-кэш).
3. **Git:** `git add` только конкретных файлов (никогда `-A`/`.`); перед коммитом `git diff --cached --stat`
   (чужие pre-staged); **DCO `git commit -s`** (CI-гейт); коммит после каждой задачи; sync в main после
   фазы (bidirectional: pull main → ветка, merge ветка → main).
4. **Rate-limit устойчивость:** фоновые агенты в workflow иногда ловят серверный rate-limit и падают.
   Workflow auto-retry'ит transient; терминальные → `null`. Скрипты ОБЯЗАНЫ `.filter(Boolean)` и
   продолжать на частичном результате; идемпотентные шаги + чекпоинты (commit per task) → resume
   (`resumeFromRunId` — кэш завершённых агентов). НЕ зависеть от успеха каждого агента.
5. **Сборка:** `cargo build --release --manifest-path nova-cli/Cargo.toml` → бинарь
   `nova-cli/target/release/nova.exe`. Изменил `.rs`/`nova_rt/*` → пересобрать ДО прогона.
6. **Тесты — только C-codegen** (`nova test` / `test-build`); интерпретатор UNSUPPORTED (D274).
   `nova test` требует явный путь. Per-fix verify = targeted фикстура; полный прогон — на закрытии фазы.
7. **Гейт корректности = spec_tests + detect-фикстуры фазы + baseline-delta**; nova_tests сам по себе НЕ
   гейт (только byte-identical/delta против baseline-бинаря).
8. **Батчи полного nova_tests** (Bash-таймаут ≤10мин; полный прогон ~60-90мин): циклом по группам
   топ-папок `nova test nova_tests/<dir1> nova_tests/<dir2> … --results-file r<N>.json`, каждый батч
   <10мин; хвостом `--rerun-failed`; отдельно `nova test spec_tests` и `nova test std`. Флака ≠ регрессия
   (сверять против baseline на ТОМ ЖЕ бинаре).
9. **Disasm-baseline процедура (для §5 п.3):** ДО Ф.1 — в temp-worktree на parent-коммите Ф.1 собрать
   release-бинарь, скомпилировать ДВЕ фикстуры: (a) Mutex/Semaphore/`Consumable[Never]`-hot-path
   (после Ф.2-rename — `Cleanup[Never]`), (b) цепочка `Fail[E]`-fn без локального handler/defer
   (frame-free пропагация); сохранить `objdump -d` (или дамп `.c`) в артефакт
   `docs/plans/artifacts/173-disasm-baseline/`; ПОСЛЕ Ф.1, Ф.2 и Ф.4 — тот же прогон, diff:
   (a) число setjmp-кадров/вызовов hot-path не выросло, (b) пропагационная цепочка = **0 setjmp-кадров**.
10. **Dev-логи:** после каждой фазы — project-creation.txt + discussion-log.md (nova-private) +
    simplifications.md (по dev-workflow §7).

## 7a. Запуск-чеклист («выполни план 173» разворачивается в это)

1. Прочитать план целиком + хаб + [173.0-173.3] заголовки (§8-источники).
2. Worktree + сборка (§7 п.2/п.5); прогнать targeted smoke: `nova test nova_tests/concurrency` (батч).
3. **Prerequisite-check** (стоп при незакрытом гейте → эскалация владельцу):
   - Ф.0R/Ф.1/Ф.2 — гейтов нет, выполнять сейчас;
   - Ф.3-семейство — по порядку 173.0 → 173.1 → 173.2 → 173.3 (у каждого свои гейты внутри);
   - §3a п.3 deadline/timeout — Plan 175 (📋 READY, не начат);
   - Ф.4 — Plan 174.3 (📋 PROPOSED, не начат) — реализуй первым;
   - Ф.6 — Ф.1 + Ф.5-`nova_runtime_reset`.
4. Порядок: Ф.0R → Ф.1 → Ф.2 → (173.0 → 173.1 → 173.2 → 173.3 + Ф.3-остаток) → Ф.4 → Ф.6; Ф.5 — сквозная.
5. Каждая задача: код → targeted тест → commit (-s) → лог; фаза: полные гейты §5 → sync main.

---

## 8. Источники для исполнителя (контекст)

**Хаб:** [docs/idiom/error-and-cleanup-model.md](../idiom/error-and-cleanup-model.md).
**Конвенции (нормативные для исполнения):** [test-conventions.md](../test-conventions.md),
[dev-workflow.md](../dev-workflow.md), [conventions-governance.md](../conventions-governance.md),
[compiler-conventions.md](../compiler-conventions.md).
**Беклоги (ДВА файла):** OPEN-view индекс [docs/plans/backlog-followups.md](backlog-followups.md);
детальные маркеры [docs/backlog-followups.md](../backlog-followups.md) (`[M-172-with-fail-swallows-panic]`
:185, `[M-172-errdefer-okdefer-dead-surface]` :169 — file:line внутри них stale, актуальные в §1).
**Суб-планы:** [173.0](173.0-concurrency-runtime-substrate.md), [173.1](173.1-parallel-collect-and-supervised-value.md),
[173.2](173.2-supervision-as-effect.md), [173.3](173.3-data-race-freedom-share.md); смежные
[174.2](174.2-question-mark-return-only.md), [174.3](174.3-any-type-and-is-downcast.md),
[175](175-time-system-rework.md), [169.1.1](169.1.1-test-lane-flags-and-ci.md)/[169.1.2](169.1.2-consolidate-tests.md).
**Спека (авторитет):** `spec/decisions/08-runtime.md` (D13), `04-effects.md` (D85, NovaFailFrame, stale ## D4),
`03-syntax.md` (D90/D158/D160/D161/D188/D189/D190/D194/D196/D197), `06-concurrency.md` (D14/D50/D75),
`09-tooling.md` (D89/D298).
**Код (истина; строки актуальны на 2026-07-03):** `compiler-codegen/nova_rt/effects.h` (`nv_panic` :555,
`NovaFailFrame` :55-64, throw-семейство :93-131, `rethrow_with_suppressed` :210-218, suppressed-аксессоры
:269-283, exit-timeout-заглушка :256-260), `compiler-codegen/src/codegen/emit_c.rs` (with-Fail re-dispatch
:6885-6933, defer/on_exit :17613-19860, `?`/`!!` :21895-22050, parfor :8280-8411, DeferKind :1322,
type_id :1241/:1249), `compiler-codegen/src/parser/mod.rs` (D189-reject :10054-10090, supervised :9764-9800),
`compiler-codegen/src/types/mod.rs` (D133-quickfix :18764+), `ast/mod.rs:1849-1865`,
`std/prelude/core.nv:147` (ScopeOutcome), `std/prelude/errors.nv:199-250` (MultiError),
`std/concurrency/cancellation.nv` (with_timeout/race2).
**ПРЕДУПРЕЖДЕНИЕ:** область противоречива — НЕ доверять одному файлу/summary; код = истина, спека местами
stale. Верифицировать, а не верить. Строки дрейфуют — проверяй grep'ом перед правкой.

## 9. Followup-маркеры

`[M-173-error-system]` (umbrella) + `[M-173-panics-clause]` (Ф.6) + `[M-173-error-return-trace]`
(полный propagation-trace; минимум — Ф.5 п.7) + `[M-parfor-record-result-miscompile]` (home —
[173.1](173.1-parallel-collect-and-supervised-value.md); interim-guard — Ф.1 п.5).
Декомпозиция Ф.3 УЖЕ выполнена (2026-06-21) → семейство 173.0-173.3. Ф.4 остаётся в зонте (гейт 174.3).
