# Plan 173 — Система ошибок и cleanup: унификация + hardening (panic/fail/defer/on_exit), production-grade

> **Top-level umbrella-план.** Создан 2026-06-20. **Ред. 2 — 2026-07-03**: полная сверка (5-агентный аудит-workflow):
> ground-truth дефектов перепроверен по текущему коду (11/11 живы, строки актуализированы), противоречия
> шапки/§4/§6 с owner-пересмотрами §3a/§3b устранены, планка расширена до **7 языков (+Zig/Swift)**,
> тест-план приведён к test-conventions (одна папка + spec_tests-покрытие), добавлены §7a запуск-чеклист и Ф.0R.
> **Статус:** ✅ ЗАКРЫТ — все фазы Ф.0R-Ф.6 закрыты по телу файла (Ф.0R 2026-07-09, Ф.1 2026-07-04,
> Ф.2 2026-07-04/08, Ф.3-остаток 2026-07-09/10, Ф.4 2026-07-06 + хвост 2026-07-13, Ф.5 2026-07-10, Ф.6 2026-07-10).
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
| 3 | ✅ **FIXED (Ф.1, ea55bee7)** `?` строго return-only: чекер отвергает free `?` в fn с return ≠ Result/Option → `[E_TRY_IN_FAIL_FN]` (per-fn walker в `check_fn`). EXEMPT: consume-init `?` (D196 form 2 — codegen `in_fail_ctx`-ветка СОХРАНЕНА, её носит D196), defer-body `?` (D158), closure. Завершает codegen-часть 174.2 | `types/mod.rs check_fn` | ✅ |
| 4 | ✅ **FIXED (Ф.1, 84e6e709)** Мёртвый `errdefer`/`okdefer`/`defer\|result\|` surface: удалены `Stmt::ErrDefer/OkDefer/DeferWithResult` + ~90 match-arm сайтов (18 файлов), `DeferKind` enum + path-selective skip-логика (emit_c: все defer'ы плейн), D189-deprecation lint subsystem (lints). Сохранён tombstone (lexer `KwErrDefer/KwOkDefer` + parser `[D189-removed-*]`) | см. слева | ~~P3~~ ✅ |
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

> ✅ **`supervised(deadline:)` / `supervised(timeout:)` ПРИЗЕМЛЕНЫ (Plan 174, D408, 2026-07-06).**
> Обе формы + `cancel:` комбинируются; таймер→областная отмена (путь `cancel:`)→типизированный
> `TimeoutError` (prelude, ловится `is TimeoutError` / `with Fail[TimeoutError]`); USER-precedence;
> вложенность через min-комбинацию точки (`nova_scope_init` inherit + `nova_deadline_combine`); sleep
> прерывается рано; zero/past→immediate. Longjmp-safe restore `_nova_active_scope` (run_impl + with-Fail).
> Тесты `std/concurrency/supervised_deadline_test.nv` 8/8; regress delta 0. **`parallel for`-зеркалирование
> deadline-параметров — отдельный заход** (десугарит в supervised, но keyword-args ParallelFor пока нет)
> → маркер `[M-174-parallel-for-deadline]`. Известное ограничение: main-flow blocking В ТЕЛЕ до старта
> run-loop не ограничено сроком (идиома — `spawn` работу); документировано в simplifications.

**(4) `with_timeout` — убрать; `race2` — оставить до общего `race`.** ✅ **ЗАКРЫТО 2026-07-10**
(Plan 175 Ф.3a/Ф.5d landed) — `with_timeout[T]`/`within[T]` УДАЛЕНЫ из
`std/concurrency/cancellation.nv` (субсумированы `supervised(timeout:)`, D408); все реальные
call-сайты (fn + тесты) мигрированы на `supervised(timeout:)`/`race2`. Опасение «cancellation.nv
независимо сломан retired-API-дрейфом `str.len`/`ro`-field» из более раннего захода на момент
закрытия НЕ подтвердилось (файл компилировался чисто) — возможно, было исправлено отдельно
до этой волны. Маркер `[M-174-retract-with-timeout]` CLOSED.
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
3. **D-нумерация verify:** D314 зарезервирован за 173 (`wip/172.1-d-status.md:411`) — подтвердить свободу;
   Ф.6 D-блок = **D348** (D340-D346 serde, D347 Plan 181; D348+ свободны — проверено 2026-07-03). NB
   попутная находка: Plan 178 претендует на D327-D332, а D327 уже занят в спеке — коллизия НЕ наша,
   передать владельцу Plan 178 (заметка в discussion-log).
4. **Маркеры в OPEN-view:** добавить строки `[M-172-with-fail-swallows-panic]`, `[M-172-errdefer-okdefer-dead-surface]`
   (home = 173 Ф.1) и `[M-173-error-return-trace]` в `docs/plans/backlog-followups.md` (детали маркеров
   М-172 живут в `docs/plans/backlog-followups.md:169,185` — там обновить stale file:line на актуальные из §1).
5. **Хаб:** точечный статус-фикс (баннер «§3a/§3b 2026-06-26: completes-by-default, D192-ретракт,
   no-restart-default») — полный rewrite будет в Ф.2.
- **Acceptance:** 173.1/173.2 не содержат отменённых knobs; D314/D348 подтверждены; маркеры в OPEN-view;
  **без упрощений**.
- ✅ **Ф.0R ЗАКРЫТА 2026-07-09** (sonnet, ветка `err-173-f3`, `339ceeff4`): п.1 (173.1 — `strategy:`/
  `max_restarts:`/`period:` убраны из параметр-сета) / п.2 (173.2 — SUPERSEDED-врезка §3b: MVP Escalate/Stop,
  Restart за гейтом изоляции; п.6/п.7 гард расширен до shared-mut) / п.3 (D314 подтверждён spec-first
  03-syntax.md:10837; D348 в спеке отсутствует → свободен за Ф.6; **NB коллизия Plan 178/D327 УЖЕ
  саморазрешена — 178 перенумеровал HTTP на D357-D362, D327=codepoint/172.2; передавать нечего**) /
  п.4 (`[M-173-error-return-trace]` добавлен в OPEN-view P1-таблицу; два M-172 уже были ✅ FIXED) /
  п.5 (хаб `error-and-cleanup-model.md` — §3a/§3b-баннер в статус-блок). Заметка в discussion-log (nova-private).

### Ф.1 — Soundness + hygiene (СЕЙЧАС, обязательно; design-risk ноль)
Багфиксы, не зависящие от деталей Model 1:
1. **#1 fix:** `with Fail` ре-throw'ит PANIC (ветка перед CANCEL/USER, `emit_c.rs:6885+`); сразу ввести
   общий helper `nova_scope_exit` (предшественник Ф.2 п.4). Закрывает `[M-172-with-fail-swallows-panic]`.
2. ✅ **#3 fix РЕАЛИЗОВАН (ea55bee7, 2026-07-04) по Option A — премиса плана СТАЛА, переопределена:**
   `?` строго return-only + `[E_TRY_IN_FAIL_FN]` (per-fn walker `check_try_return_only_*` в `check_fn`;
   consume-init/defer-body/closure exempt; 2 free no-op-`?` сайта мигрированы; D196 in_fail_ctx-ветка
   СОХРАНЕНА). Завершает codegen-часть [174.2](174.2-question-mark-return-only.md). Гейт: conformance 38/38
   (D196/d158 не сломаны), pos/neg err173/f3_*, spec D85 amend + D4/дубль banners + `@commit()?`→`!!`.
   *(Ниже — исходная де-риск-запись, оставлена как обоснование Option A.)*
   **НАХОДКА де-риска (blast-radius по всему корпусу — 15 сайтов postfix-`?`, из них релевантны 5):**
   174.2-премиса «throw-режим не задействован, codegen-ветку убрать как недостижимую» — **СТАЛА**.
   Категории: **(KEEP)** free `?` на Result/Option-возвращающих fn (return-mode): `effects/throws.nv`,
   `effects/error_chains.nv` — НЕ трогать. **(MIGRATE, 3 сайта)** free `?` на **unit** внутри Fail-fn
   (`process2() Fail -> ()` + два `d158_*`): `?` на unit хитит codegen no-op-ветку `/* ? */` — бессмыслен;
   миграция = **дропнуть `?`** (Fail пробрасывается эффектом сам). **(EXEMPT, 2 сайта)** `consume X = expr? {}`
   — **D196 form 2** (Result-unwrap init, `check_consume_unwrap_form.nv`, conformance `d196_consume_scope_init_forms.nv`):
   `?` на Result внутри Fail-fn → это ЕДИНСТВЕННЫЙ реальный носитель `in_fail_ctx` throw-ветки
   (`emit_c.rs:22092-22145`). **РЕШЕНИЕ (Option A, консервативно — уважает D196-дизайн владельца):
   EXEMPT consume-init `?`; codegen throw-ветку СОХРАНИТЬ (D196 её использует — план «убрать ветку»
   АННУЛИРОВАН находкой); чекер отвергает только FREE-standing `?` в fn с return-type ≠ Result/Option.**
   Причина против Option B (мигрировать D196 form 2 `?`→`!!`): владелец выбрал `?` для consume-init
   намеренно; смена канона D196 — его spec-решение, не рефактор в хвосте 173.
   **Impl-план (next):** (a) helper `return_is_result_or_option(fd.return_type)`; (b) per-fn walker в
   `check_fn` (`types/mod.rs:4790`, есть `fd.return_type`) — рекурс по stmt/expr, для `ExprKind::Try`
   (кроме top-level `init` у `ConsumeScope` — D196-exempt) при !Result/Option → `[E_TRY_IN_FAIL_FN]`
   (hint «используй `!!`/`throw`»); закрытие closure-контекста если у lambda свой ret; (c) мигрировать
   3 no-op сайта (дроп `?`); (d) тесты `nova_tests/err173/` (pos `?` на Result→return Err + на Option→
   return None; neg free `?` в Fail-fn → `[E_TRY_IN_FAIL_FN]`; pos D196 consume-init `?` не задет); spec:
   amend **D85** (`?` return-only + note consume-init-exempt), stale `## D4`/дубль `####` (04-effects:290/950),
   doc-примеры `@commit()?`→`!!`. **NB codegen throw-ветку 22092-22145 НЕ удалять** (D196 form 2 её носит).
3. **#2 fix:** диагностика D133 — quick-fix на `defer`/`@cleanup` вместо errdefer/okdefer
   (`types/mod.rs:18764+, 18819+`, D162-quickfix `:19636+`).
4. ✅ **#4 fix (84e6e709):** удалён мёртвый errdefer/okdefer/defer|result| surface (AST-варианты +
   ~90 match-arm сайтов в 18 файлах, `DeferKind` enum + path-selective skip-логика, D189-deprecation
   lint subsystem); tombstone-распознавание для D189-hint сохранено (lexer + parser). Закрыл
   `[M-172-errdefer-okdefer-dead-surface]` (все 3 слоя, вместе с дефектом #2).
5. ✅ **interim-guard #7-concurrency РЕАЛИЗОВАН (7514b262):** чекер отвергает `parallel for → []T`
   в value-позиции с непримитивным элементом (T ∉ {int,bool,f64,str}) чистым `[E_PARFOR_RESULT_UNSUPPORTED]`
   (per-fn/-test walker `check_parfor_result_*`, отдельный проход после f1; value-position через `consumed`;
   statement-mode не задет). §0: whitelist в const + bidirectional coupling-коммент с codegen (emit_c.rs:8492).
   Закрыл `[M-parfor-record-result-miscompile]` (silent-degrade → чистая диагностика). Снимается 173.1 Ф.2.
- **spec/docs:** `## D4` + дубль (:950) → RETRACTED-баннер; `spec/decisions/README.md` строки 18/19/36 fix;
  хаб — статус-фикс (Ф.0R п.5 если не сделан).
- **Тесты** (раскладка — §4a): pos `rt/f1_with_fail_swallow_panic.nv` (panic сквозь `with Fail` → процесс
  падает с `panic:`); pos `f1_try_return_only` (`?` на Result/Option). neg: `?` в Fail-fn →
  `[E_TRY_IN_FAIL_FN]`; `errdefer{}`/`okdefer{}` → `[D189-removed-*]`; непримитивный `parallel for`-результат
  → `[E_PARFOR_RESULT_UNSUPPORTED]` (не сырой C-error). spec_tests/conformance: d85/d13-покрытие затронутого.
- **Acceptance:** ✅ **Ф.1 ЗАКРЫТА 2026-07-04.** #1 (25e07590) / #2 (4a02c825) / #4 (84e6e709) /
  #3 (ea55bee7, переопределён по де-риску — Option A, D196-exempt) / interim-guard #7 (7514b262) —
  все закрыты; conformance 38/38 на каждом атоме; nova_tests baseline-delta = 0 (default + --panic lane);
  spec D85/D4/D71 amended; тесты `nova_tests/err173/` (pos/neg peer f1/f3/f7 + D189-removed neg).
  **Interim-статус #7** (parfor-guard) — plan-sanctioned stopgap до 173.1 Ф.2 (§0 coupling задокументирован).
  disasm hot-path: touched-подсистемы baseline-only (checker-changes не трогают codegen). **Без упрощений**
  (кроме явно-interim #7). **Следующее: Ф.2 (defer-kernel unification, spec-first D314) ИЛИ Track A (172.12/172.13).**

### Ф.2 — Унификация defer-kernel (✅ ЗАКРЫТА 2026-07-04; см. де-риск-карту §«СВОДКА»; исполнение-подтверждение 2026-07-08)

> 🔨 **Ф.2.0 ЗАКРЫТ (D314-spec + де-риск-карта):** D314 написан spec-first в
> [spec/decisions/03-syntax.md](../../spec/decisions/03-syntax.md#d314); полная де-риск-карта
> (ultracode Workflow, 4 агента) + **скорректированная последовательность под-атомов** —
> (чекпоинт волны удалён при закрытии, см. git-историю). **🔴 КРИТИЧЕСКИЕ НАХОДКИ де-риска:**
> (1) **D194-элизия §3.5 ПРЕМИСА ЛОЖНА** — код НЕ элидит (полный frame-bearing путь безусловно);
> Ф.2 acceptance = PARITY (не регрессировать), §perf-элизия → followup `[M-173-d194-perf-elision]`.
> (2) **`nova_scope_exit` нужен policy-параметр** `{CATCH,TRANSPARENT}` (with-Fail USER→swallow vs
> defer/consume USER→rethrow); 7 kind-сайтов, C2 hand-rolled-longjmp + fiber-report — miss-risk.
> (3) **rename collision:** эффект `Cleanup`→`ResourceTrace` ПЕРВЫМ (иначе prelude duplicate-def).
> (4) **interrupt-outcome** = `Failure` (core.nv:130; desugar выравнивает impl к спеке). Порядок
> под-атомов (каждый = заход+коммит+гейт): A0-baseline → R1-ResourceTrace → R2-Consumable/Cleanup →
> B1-parser/AST defer(o) → B2-codegen outcome → B3-consume-desugar → C-nova_scope_exit → D194-parity →
> E-hub-rewrite. **⚠ План-строки ниже (п.1-10) дрейфанули — верить де-риск-карте, всегда re-grep.**

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
- ✅ **Закрыта.** Все под-атомы (A0/R1 43f9ee5bd/R2 ffb76506e/B1 e0d95a313/B2 23f512d84/B3-merge a23579f3e
  (разблокирован D314 §4a `501adb50e`)/C `c6254274e`/D194 `66a29f63a`/E `0636a9edd`) уже в `main` —
  подтверждено де-риск-картой (чекпоинт волны удалён при закрытии, см. git-историю; §«СВОДКА Ф.2», ПОЛНОСТЬЮ ЗАКРЫТА). Исполнение-заход
  2026-07-08 (sonnet, ветка `defer-kernel-173-f2`, worktree nova-p173) — новых пунктов не осталось,
  подтверждающий прогон гейтов: `cargo build --release` оба крейта чисто; conformance
  `spec_tests/conformance` **70/0**; defer/errdefer/cleanup корпус (err173, err173_0, plan110, plan103_9,
  plan100_4_1/2/4/5, plan125_1) **54/1** (единственный CC-FAIL `plan125_1/neg/let_never_no_context` —
  pre-existing, задокументирован в де-риск-карте Ф.2.R2); `std/http`+`std/io`+`std/fs` **15/0** (+1 SKIP
  `servernet` — нет test-блоков, ожидаемо). Followups вне периметра остаются: `[M-173-consume-exactly-once-observability]`,
  `[M-173-d194-perf-elision]`, `[M-173-consume-unwind-cleanup-throw]`, multi-binding consume (п.6).

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
- ✅ **Ф.3-остаток ЗАКРЫТ 2026-07-09** (sonnet, ветка `err-173-f3`; D-блок = **D414** — high-water был D413,
  следующий свободный за D348-Ф.6): **п.1** (`304985107`) precedence PANIC>USER>CANCEL — единый
  `nova_throw_kind_precedence` в обеих report-точках (local+M:N CAS), D13-соундность; D414 §1 + EXPECT_RUNTIME_PANIC
  тест ×5. **п.2** (`1b2d835ba`) detach enforce `Detach`-эффекта (`[E_DETACH_REQUIRES_EFFECT]`, CapabilityCtx;
  exempt test-root/ambient-handler/handler-op-полиморфизм) + error-policy словарь (LogAndDrop default /
  escalate-to-scope opt-in `[M-173-detach-escalate-to-scope]`); D414 §2; neg+pos тесты. **п.3** (`993b8c35b`)
  recv→Option канон верифицирован + **select `None`-арм РЕАЛИЗОВАН** (различает closed от value:
  `SelectSlot.want_none`, parser/AST/codegen/runtime); D414 §3 + D94 update; тесты err173_2. **п.4** (`999bffff9`)
  переписаны stale-тесты `supervised_errors.nv`/`fiber_throw.nv` SECTION 4 (ложь «throw неперехватываем» →
  реальные тесты, верифицированы в изоляции 3/3; concurrency-folder заблокирован pre-existing Duration-багом
  Plan175 — вне периметра). **п.5 ЗАКРЫТ 2026-07-10** (гейт Plan 175 Ф.3a/Ф.5d landed —
  Monotonic мокабельность + Duration/typed surface): `within[T]`/`with_timeout[T]` УДАЛЕНЫ из
  `std/concurrency/cancellation.nv` (субсумированы `supervised(timeout:/deadline:)`, D408);
  `race2[T]` остаётся (не субсумирован). Мигрированы вызовы: `nova_tests/concurrency/
  cancellation_test.nv` (4 within-теста удалены, race2-тесты сохранены),
  `mn_closure_spawn_gcroot_test.nv` (последний тест → `race2` вместо `within`, тем же путём
  починен НЕЗАВИСИМЫЙ pre-existing mut-capture баг в `run_int`-хелпере — D415 §2 дрейф,
  найден при миграции, починен той же волной), `examples/real_world/audit.nv` (иллюстративный,
  сам файл всё равно pre-existing CODEGEN-FAIL по несвязанным причинам — вне scope). Маркер
  `[M-174-retract-with-timeout]` CLOSED. spec: 08-runtime.md:168 stale-стратегии обновлены;
  хаб + D414 §1-§3. **Без упрощений.**

### Ф.4 — MultiError end-to-end + типизированный ScopeOutcome (✅ ЗАКРЫТА 2026-07-06; ГЕЙТ [Plan 174.3] ✅ в main)
1. ✅ **#6 РЕАЛИЗОВАН (2026-07-06, модель Б — решение владельца):** противоречие спеки D158 (конверт
   `Err(MultiError{primary,suppressed})`) и §6 разрешено В ПОЛЬЗУ §6 — primary отдаётся ловящему
   **КАК ЕСТЬ** (типизированная ловля работает, эффект НЕ становится `Fail[MultiError]`); подавленные —
   «в кармане», достаются свободным аксессором **`suppressed() -> []any`** ПОСЛЕ ловли
   (`std/prelude/runtime.nv` + re-export в facade). Карман = thread-local
   `_nova_last_error.frame.error_suppressed` (инфра #5); заполняется (a) FAIL-path — зеркалирование в
   `nova_rethrow_with_suppressed` (transport-chokepoint), (b) interrupt-path (доминирующий идиом
   `with Fail = … interrupt`) — per-cleanup `NovaFailFrame` вокруг каждого defer-тела с prepend-compose
   прямо в карман (panic cleanup'а НЕ глотается — `nv_panic`; snapshot+relink головы сохраняет
   аккумуляцию сквозь `nova_last_error_set`-reset бросающего cleanup'а). Материализация `[]any`:
   codegen-intercept `suppressed()` (emit_c.rs `emit_call`) через ГОТОВЫЕ read-аксессоры
   `nova_failframe_suppressed_count/at`; элементы `any` (typed payload — `nova_any_from_boxed`, голый
   str — `nova_any_box`), сужение `is T`/`.try_as[T]()`. Спека D158 амендирована (баннер +
   §«Модель доставки — вариант Б» + §«Что отвергнуто»); тип `MultiError` (`errors.nv`) остаётся
   опциональной value-обёрткой, НЕ конвертом эффекта.
   **Сопутствующий (той же осью) фикс hijack'а:** cleanup-throw во время unwind РАНЬШЕ диспатчился в
   ещё-установленный with-Fail handler сцены (string-slot arm без tid-check → мисфайр на чужом payload,
   перезапись in-flight результата, двойной прогон defer-тела). Модель Б: `NovaFailFrame.is_cleanup`
   (ставится codegen'ом на unwind-path cleanup-кадрах: defer FAIL `_tdf` / interrupt `_idf` /
   consume FromFrame|Interrupt) + `nova_in_cleanup_unwind()`-байпас в `Nova_Fail_fail`/
   `nova_throw_typed`/generated per-E entries — ошибка cleanup'а летит в свой кадр → карман; явный
   handler-wrap ВНУТРИ cleanup (свой не-cleanup кадр) работает как раньше (D158 backward-compat);
   normal-exit cleanup-кадры не маркируются (их ошибка = primary, handler срабатывает). Попутно:
   `nova_interrupt_push_defer` зануляет `value`/`value_ptr` (re-issue пробует value_ptr — stack-garbage
   уводил int-interrupt в pointer-роут).
2. ✅ **#5 РЕАЛИЗОВАН (2026-07-06):** `ScopeOutcome.Failure(any)` (`core.nv:149`) — типизированный payload
   протянут в outcome; `if e is T` narrowing в `@cleanup`/`defer(o)` (D54/174.3). Materialization
   (`assign_scope_outcome_from_frame`, `emit_c.rs`): USER_TYPED → `nova_any_from_boxed(payload,tid)` (усыновляет
   throw-site heap-box); CANCEL → typed `CancelError{reason}` box (`err is CancelError`, префикс `"cancel: "`
   убран); USER(bare str) → `any=str`; interrupt → `any=str "interrupt"`. **Ключевая находка:** в доминирующем
   идиоме `with Fail = … interrupt` handler срабатывает на throw-site, scope выходит через interrupt, а
   `_nova_fail_top` к моменту cleanup УКАЗЫВАЕТ НА РАЗРУШЕННЫЙ stack-fail-frame (cross-fn throw → segfault). Fix:
   thread-local **STABLE snapshot `_nova_last_error`** (`effects.h`), стемпится на throw (`nova_throw*`/
   `nova_throw_typed`/`Nova_Fail_fail`/`nv_panic`/assert-panic), читается cleanup'ом на interrupt-path, гасится
   на catch (`nova_scope_exit CATCH` + with-block-consume в `nova_interrupt`). Закрыл `[M-110-multierror-any]`
   в части `ScopeOutcome.Failure(any)` (typed cleanup narrowing); типизация полей самого `MultiError`
   (`primary`/`suppressed` `str`→`any`) остаётся за #6.
   Тесты: `err173/f4_typed_scope_outcome.nv` (typed/str/is-discrim, PASS); migrated `plan110/…_t2_11` на
   `err is CancelError`. Conformance 53/0; regress delta 0. **Остаток (документирован в simplifications):**
   value-typed throw box-repr предполагает pointer (records — универсум typed-errors); per-E-handler+interrupt
   staleness-окно (payload-only, already-Failure).
3. ✅ **#7 РЕАЛИЗОВАН (2026-07-06):** инвариант suppressed-chain переосмыслен под модель Б — reset
   `error_suppressed=NULL` на СВЕЖИЙ throw = ПРАВИЛЬНАЯ семантика (новая ошибка = новый карман; нет
   утечки между несвязанными ловлями); cleanup-throw'ы НЕ «голые» — каждый локально кадрирован
   (is_cleanup-кадр) и композируется (FAIL-path: chain на scope-кадре → `nova_rethrow_with_suppressed`;
   interrupt-path: prepend в карман напрямую). Порядок = хронология аварий (LIFO-цепочка читается
   back-to-front). Дыра «per-E entry не стемпит `_nova_last_error`» закрыта (stamp в начале generated
   `_nova_throw_typed_<E>` — иначе per-E-ловля не сбрасывала карман → чужая цепочка текла в следующую).
   Тесты: порядок [C,B] при двух авариях; пустой карман; переживание промежуточного кадра;
   catch+rethrow; несвязанные ловли не смешиваются.
4. typed-предикатный доступ: **`e is T`** (D54-семантика, инфра 174.3); `.downcast[T]()` НЕ вводится (§6).
5. *(surface-вопрос закрыт — §6: имена = `errors.nv`, typed-доступ = `is`.)*
6. ✅ **Инвариант catchability РЕАЛИЗОВАН + под regression-guard (2026-07-06):** primary остаётся
   носителем; `with Fail[Primary]` СРАБАТЫВАЕТ (тест `run_typed_catch`: тело кидает `SupA`, defer кидает
   `SupB` → typed-ловля получает `e.code`, эффект НЕ `Fail[MultiError]`, `suppressed()==[SupB]`);
   catch+rethrow сохраняет карман (`run_catch_rethrow`). (Упала ТОЛЬКО cleanup → primary =
   cleanup-ошибка, карман пуст — normal-exit кадры не маркируются, handler срабатывает.)

**Фундамент typed-errors — type_id-инфра Plan 61 (готова):** compile-time `NOVA_TID_<E>`
(`type_id_registry`, `emit_c.rs:1241`), typed-throw несёт `(payload, tid)`, матчинг в arm'е handler'а
(`fail_e_map`, `emit_c.rs:1249`) → `is T` = та же проверка `type_id == NOVA_TID_T`. `any`-boxing/vtable —
[Plan 174.3](174.3-any-type-and-is-downcast.md) (📋 PROPOSED, не начат — **реализуй ПЕРВЫМ**; Ф.4 полностью
заблокирована до него). Ф.4 строит на готовом фундаменте, не с нуля.
- **spec/D/Q/docs:** ✅ D158 амендирован (модель Б, 2026-07-06 — коммит 74329729); D190 (`ScopeOutcome[E]`
  остаётся rejected). Хаб-верификация + D193-текст-sweep → Ф.5 п.4 (materialization-часть выполнена здесь).
- **Тесты (выполнено 2026-07-06):** `nova_tests/err173/f4_suppressed.nv` (9 тестов): typed catchability
  (`with Fail[SupA]` ловит, `suppressed()==[SupB]`, `is`-сужение); хронологический порядок [C,B];
  пустой карман; переживание промежуточного кадра; catch+rethrow; несвязанные ловли раздельны.
  spec_tests: `conformance/d158_suppressed_pocket.nv` (3 теста — модель Б, пустой карман, no-leak).
  *(Отложено вне Ф.4: neg-тест «`with Fail[Primary]` НЕ ловит чужой тип» упирается в string-slot
  dual-install арм без tid-check — известное поведение per-E dispatch'а Plan 61, отдельная ось;
  неexhaustive-match-по-E compile-гейт — чекерная фича, кандидат Ф.5/followup.)*
- **Acceptance:** ✅ **Ф.4 ЗАКРЫТА 2026-07-06.** D158 материализация end-to-end (модель Б);
  cleanup-ошибка НИКОГДА не перезаписывает body-ошибку (и больше не hijack'ит handler/результат);
  catchability под regression-guard; conformance 54/0; regress delta 0; **без упрощений**.
- ✅ **Хвост «scope-агрегация» ДОДЕЛАН 2026-07-13** (волна «173 хвосты», ветка `tails-173`) —
  обещание Ф.3-остатка «после Ф.4 — агрегация MultiError» и D414 §1 «не-primary → suppressed»
  оставалось НЕреализованным (re-throw хвост кидал только primary; `nova_scope_collect_child_errors`
  не имел вызывающих). Теперь: не-primary retained детские падения (кроме CANCEL-производных и
  Stop-решённых D416) композируются в suppressed-карман primary-броска (staging
  `_nova_pending_suppressed` → потребление в `nova_last_error_set`); после ловли primary читаются
  `suppressed() -> []any`. Попутный ABI-фикс `nova_any_from_boxed` (value-примитивы tid 1..7:
  data = heap-box напрямую — `try_as[int]` на элементе возвращал адрес бокса). Тест
  `err173_2/scope_multierror_test.nv` (3 сценария); D414 §1-амендмент тем же слиянием.

### Ф.5 — Hygiene: exactly-once + watchdog + traces + spec-sweep (СКВОЗНАЯ: пункты закрываются по готовности зависимостей)
1. **#8:** D188 R2 exactly-once runtime-счётчик `_consume_count` + `D188-on-exit-double-invocation`
   (production-grade защита от ручного/FFI double-invoke) — НЕ сводить к структурному (упрощение).
2. **#10 (в §3a-редакции):** УДАЛИТЬ force-timeout заглушку (`effects.h:256-260`); вместо неё —
   **watchdog-варн** «fiber застрял в cleanup» при превышении порога (порог: 3-level resolution
   `WithExitTimeout` vtable → Application → default — сохранить как источник ПОРОГА варна, не прерывания);
   превышение наблюдаемо в ResourceTrace exit-событии (duration/overrun-флаг). `CleanupTimeoutError` как
   outcome НЕ существует (D192-ретракт). **⟵ СЮДА перенесён из Ф.2.R1 дроп `timeout` из
   `ResourceTrace.on_resource_enter`** (§3a/п.8 «timeout из enter убран»): в Ф.2.R1 (rename-only, 43f9ee5b)
   timeout СОХРАНЁН, т.к. его дроп ретайрит D195-Application-override-тесты (`timeout_application_level2_t3_8`,
   `application_cross_fiber_t8_7`) — делать вместе с этим timeout-rework (порог мигрирует в watchdog/scope-дедлайн).
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
- ✅ **Ф.5 ЗАКРЫТА 2026-07-10** (sonnet, ветка `err-173-f56`): **п.1** (`843810e49`) D188 R2 exactly-once —
  РЕАЛЬНЫЙ runtime-счётчик: скрытое поле `_consume_ccount` на heap-record инстансе + пролог-guard в
  `Nova_<T>_consume_cleanup` (единый chokepoint всех путей вызова, включая обход чекера через границу
  функции) → `D188-on-exit-double-invocation`; чекер `D188-r2-manual-on-exit` расширен на алиасы и
  receiver-подвыражения; extern "nova" cleanup'ы (D194 hot-path) исключены; rt-тест EXPECT_RUNTIME_PANIC +
  conformance d188 R2 ×2. **п.2+п.3** (`9c15dbe7b`) D192-РЕТРАКТ применён: тип `CleanupTimeoutError` УДАЛЁН
  (prelude+splice+`_nova_throw_cleanup_timeout_fn`); watchdog армится ТОЛЬКО вокруг cleanup-вызова
  (`nv_cleanup_watchdog_arm/disarm`), превышение = one-shot stderr-варн + `duration_ms`/`overrun` в
  `ResourceTrace.on_resource_exit` (D185 amend; timeout ДРОПНУТ из enter — D195-override тесты
  `timeout_application_level2_t3_8`/`application_cross_fiber_t8_7` мигрированы на overrun-наблюдение);
  3-level resolution сохранена как источник ПОРОГА; D192 → RETRACTED-баннер; `nv_resume_panic`→`nv_panic`
  (п.3, #9); попутно §4а: record-literal с неизвестным типом → `[E_UNKNOWN_TYPE]` (был тихий miscompile).
  **п.4** (`58fcf2975`) sweep: historical-баннеры D158/D160/D161/D162, stale race/with_timeout
  (06-concurrency/04-effects), README-индекс, хаб догнал факт (Ф.2 ЗАКРЫТА + Ф.5-строка). **п.6+п.7**
  (`cdd23a5b2`) `nova_runtime_reset()` (fibers.h; из user-кода недоступен — neg-тест) + throw-site
  трассировка (TLS `_nova_throw_site`, стемп на throw/panic/unreachable, печать `at file:line (throw site)`
  в 4 uncaught-abort ветках; rt-тесты ×2; полный trace — `[M-173-error-return-trace]`). Гейты:
  conformance 83/0; err173*+plan110+plan100_4+plan103_9 62/0.

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
- ✅ **Ф.6 ЗАКРЫТА 2026-07-10** (sonnet, ветка `err-173-f56`, `d1707363f`+`3e4505953`+`3f2262696`):
  **D348 в спеке** (09-tooling.md; свобода номера подтверждена — «(D348)»-комменты в emit_c про
  crossmodule-mangling были ошибочной ссылкой на фактический D381, исправлены) + amend D89 (шестой класс,
  EXPECT_RUNTIME_PANIC → legacy + `--panic`) + таблица layout. parser/AST `TestDecl.panics` (контекстное KW);
  codegen test-frame: инверсия, PANIC-дискриминатор (`error_kind==NOVA_THROW_PANIC` / не-`exit(`-префикс),
  substring `nova_test_msg_contains` (ptr,len), `nova_runtime_reset()` в эпилоге. **Миграция: 67
  panics-тестов в 62 файлах; −52 CU** (39 фолдов из neg//rt/ в folder-module + 13 expected_runtime →
  новая папка `runtime_panics/` одним CU; 6 in-place standalone). Факт-граница ýже плановой оценки −78:
  «паники» throw-класса (sync unlock-guards, Channel capacity, select closed — nova_throw USER),
  file-режимные (`CONTRACTS off`, `#unchecked`), процессные (fiber stack overflow SEH, token-bind abort,
  uncaught-trace stderr) и мигранты pre-existing-красных CU (plan153_4/5, plan138/_2, plan83_10, strings,
  contracts, plan11_followup, plan153_2 — verified родным baseline-бинарём) возвращены в legacy (55 файлов
  EXPECT_RUNTIME_PANIC). **Вскрытый §4а-фикс (D13/D414):** supervised re-throw ДЕГРАДИРОВАЛ панику ребёнка
  до ловимого USER (plain nova_throw) — теперь kind==PANIC транспортируется `nv_panic`'ом.
  test-conventions.md (a/b/c) с «· согласовано»; Rust-тесты раннера `d348_panics_clause.rs` 7/7
  (мета-FAIL-кейсы). Гейты (слито с main): cargo build чисто; conformance 83/0; err173* 28/0; --panic lane
  53/1 (TIMEOUT pre-existing); nova_tests-выборка δ=0. Маркер `[M-173-panics-clause]` ЗАКРЫТ.

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
**Конвенции (нормативные для исполнения):** [test-conventions.md](../dev/test-conventions.md),
[dev-workflow.md](../dev/dev-workflow.md), [conventions-governance.md](../dev/conventions-governance.md),
[compiler-conventions.md](../dev/compiler-conventions.md).
**Беклоги (ДВА файла):** OPEN-view индекс [docs/plans/backlog-followups.md](backlog-followups.md);
детальные маркеры [docs/plans/backlog-followups.md](backlog-followups.md) (`[M-172-with-fail-swallows-panic]`
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
