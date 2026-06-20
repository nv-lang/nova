# Plan 173 — Система ошибок и cleanup: унификация + hardening (panic/fail/defer/on_exit), production-grade

> **Top-level umbrella-план.** Создан 2026-06-20. **Статус:** 📋 READY — Ф.0 закрыта (sign-off 2026-06-20); Ф.1 можно стартовать.
> **Маркер:** `[M-173-error-system]`. **Запуск:** «**выполни план 173**» (план самодостаточен).
> **Объединяет** бывш. Plan 175 (error-system) + Plan 173 (panics-clause = Ф.6) под номером 173 (2026-06-20).
> **Decision-gate: Ф.0 ЗАКРЫТА (sign-off 2026-06-20).** Model 1 (defer-kernel); supervisor — все 3 стратегии
> (`OneForOne`/`OneForAll`/`RestForOne`), `period` обязателен (`Duration`), синтаксис
> `supervised(strategy:…, max_restarts:…, period:…, cancel:…)`. План готов к запуску («выполни план 173»).
> **Включает** panics-clause (Ф.6, бывш. отдельный Plan 173). **Завершает** [174](174-question-mark-return-only.md)
> (`?`-return-only), баги `[M-172-with-fail-swallows-panic]`, `[M-172-errdefer-okdefer-dead-surface]`.
> **Источник:** deep-analysis workflow 2026-06-20 (4 агента: ground-truth re-verify spec+код, concurrency,
> cross-lang Go/Rust/TS/Kotlin/Java, разбор идей владельца). **Хаб:** [docs/idiom/error-and-cleanup-model.md](../idiom/error-and-cleanup-model.md).
>
> **ОБЯЗАТЕЛЬНЫЙ сквозной критерий приёмки: «без упрощений, как для прода».** Ни одна фаза не
> закрывается заглушкой/симплификацией; фазирование — это ПОРЯДОК, не урезание объёма. Всё доводится
> до production-grade; «позже» ≠ «опционально».

---

## 1. Зачем (текущая система плохая — подтверждено ground-truth)

Корень: модель — **ОДИН longjmp-транспорт** (`NovaFailFrame`-цепочка на `_nova_fail_top`,
`effects.h:55-64`), общий для `throw`/typed-throw/`panic`/`cancel`/`assert`/contract/overflow,
различаемый ТОЛЬКО полем `error_kind` (USER/USER_TYPED/CANCEL/PANIC); `interrupt` — отдельный стек
`NovaInterruptFrame`. Поверх — **ТРИ несведённые поверхности** (`defer`, `Consumable.on_exit`/`consume{}`,
`with Fail[E]`), оставшиеся от **двух непримирённых эпох** (старая `defer`/`errdefer`/`okdefer` + `?`-через-Fail;
новая `Consumable`+`ScopeOutcome`). Каждый setjmp-кадр **сам** ре-диспатчит по `error_kind` → дрейф политик,
дублированный codegen, и реальные баги.

**Подтверждённые дефекты (file:line):**

| # | Дефект | Где | Sev |
|---|--------|-----|-----|
| 1 | `with Fail[E]` **глотает panic** (D13 violation): catch-ветка ре-throw'ит только `NOVA_THROW_CANCEL`, PANIC проваливается в USER-path → unit-результат, выполнение продолжается | `emit_c.rs:6646-6691` | **P1 soundness** |
| 2 | Диагностика `D133-not-consumed` строит machine-applicable quick-fix с retracted `errdefer` → код, который парсер реджектит | `types/mod.rs:15306-15318` | **P1 user-facing** |
| 3 | `?` Fail-context throw-mode (D165) ещё живёт в codegen (Plan 174 не завершён) | `emit_c.rs:21264-21327` | **P1** |
| 4 | Мёртвый `errdefer`/`okdefer`/`defer\|result\|` surface: AST-узлы, лексер-токены, ~600 строк недостижимого codegen | `ast/mod.rs:1842-1865`, `lexer/*`, `emit_c.rs:17518-18093,1228` | P3 hygiene |
| 5 | `ScopeOutcome.Failure` = `str` в коде vs `any` в спеке → типизированный error-dispatch в `on_exit` невозможен (`nova_make_ScopeOutcome_Failure` дропает payload/type_id) | `core.nv:147` vs `D188:8101`, `emit_c.rs:19312` | P2 |
| 6 | `MultiError` **никогда не материализуется** в user-код: suppressed-chain строится и пробрасывается, но НИ ОДИН codegen его не читает в Nova-значение (D158/D193 обещание не выполнено) | `emit_c.rs` write-only (17631/17668/17821/19341) | P2 |
| 7 | Suppressed-chain **теряется** на голом `throw`/cancel/typed во время unwind (`error_suppressed = NULL`) | `effects.h:93,113,131,801` | P2 |
| 8 | D188 R2 exactly-once **runtime-счётчик не реализован** (`_consume_count` нет нигде) | — | P2 |
| 9 | `nv_resume_panic` (спека D188/D197) — фикция; код зовёт `nv_panic` | `D188:8145` vs `emit_c.rs:19394` | P3 |
| 10 | exit_timeout 3-level — **заглушка** (хардкод 5000ms; Level 1/2 = TODO) | `effects.h:256-260` | P2 |
| 11 | **Stale спека:** `## D4` (+ дубль `####`) противоречат D85; D90/D158/D160/D161 описывают errdefer/okdefer как живые; README-индекс держит D4/D67 как live | `04-effects.md:290,950`, `03-syntax.md:4501+,6379,6596,6829`, `README.md:19` | P2 |

**Concurrency-дыры (прямой ответ на вопрос «как ловить/обрабатывать ошибку в spawn/supervised/detach/parallel-for»):**

| Конструкция | Текущее | Дыра |
|---|---|---|
| `spawn`/`supervised` | throw ребёнка ловится внешним `with Fail` (scope-гранулярность); panic неперехваченный → abort; первая ошибка → siblings cancel cooperatively (USER beats CANCEL) | **нет supervisor-стратегий** (OneForOne/max_restarts заявлены 08-runtime.md:168, парсер берёт только `supervised(cancel:…)`, `parser/mod.rs:9529`); **нет per-fiber catch** (first-wins, нельзя узнать какой fiber упал) |
| `detach` | orphan-fiber: panic/throw = LogAndDrop в stderr | **не перехватываемо вообще**; Detach-эффект не enforced (06-concurrency.md:919) |
| `parallel for` | десугар в supervised+spawn (`emit_c.rs:8040`) | при ошибке ребёнка `[]T`-результат отбрасывается (**all-or-throw, нет per-element `Result`**) |
| `blocking{}` | block-form ретракнут (D172); V1 leaf не должен throw'ить (undefined) | ошибки только через `Result`-возврат |
| `channels`/`select` | ошибка канала = ЗНАЧЕНИЕ (`recv→Option`, None на closed) | Some vs None-closed неразличимы (`select_closed_test.nv:29`) |

---

## 2. Планка «не хуже Go/Rust/TS/Kotlin/Java»

**Nova уже выигрывает (СОХРАНИТЬ как инвариант приёмки):**
- 3-уровневая таксономия катастроф (panic = смерть **fiber'а**, не процесса как Go; exit = процесс) — чище Go (где unrecovered panic убивает ВЕСЬ процесс).
- **panic ЗАПУСКАЕТ cleanup** (defer + on_exit срабатывают через fail-frame) — строго лучше Rust (`panic=abort` и double-panic пропускают `Drop`).
- `on_exit(ScopeOutcome)` унифицирует Success/Failure/Panic/Cancel в одном exhaustive-match — ни один из 5 языков не выражает это одной конструкцией.
- `MultiError` (primary + suppressed[]) — бьёт Go (плоский `errors.Join`), на уровне Java `getSuppressed`.
- keyword-level структурная конкурентность — впереди Go (errgroup-библиотека, first-error) и TS (Promise.all без отмены).
- cancel как структурный `Failure(CancelError)` + cancel-shield — принципиальнее Kotlin `CancellationException` (которую можно проглотить generic-catch'ем).
- effect-row `Fail[E]` делает множество ошибок видимым (как хотели checked-exceptions) + `?`-эргономика.

**Nova под риском (ОБЯЗАНЫ закрыть, иначе ХУЖЕ всех 5):**
1. **with-Fail глотает panic** — утечка tier-1, хуже ВСЕХ пяти (все запрещают ловить unrecoverable как обычный flow). → Ф.1.
2. Дочерняя ошибка в `parallel for`/`supervised` обязана **по умолчанию отменять siblings И агрегировать** в MultiError (гарантия Kotlin coroutineScope / Java `Joiner.allSuccessfulOrThrow`), а не first-error-wins как Go errgroup. → Ф.3.
3. Конверсия ошибки при пробросе не должна терять cause (анти-Go `%w`-забыл; анти-Rust `From` без `source()`). → Ф.4 (`?` уже решён 174 — explicit `.map_err`; на эффект-стороне — MultiError-цепочка).
4. Cleanup-ошибка не должна **молча перезаписывать** in-flight ошибку тела (анти-Go defer-overwrite) — только compose в suppressed. → Ф.2/Ф.4.

**Референс для concurrency-API:** Java **JEP 533 `StructuredTaskScope`/`Joiner`** (allSuccessfulOrThrow / anySuccessfulResultOrThrow / awaitAll) + Kotlin `coroutineScope`/`supervisorScope`.

---

## 3. Рекомендованный дизайн — MODEL 1 «defer — это ядро»

Единый примитив `defer` с опциональным outcome-биндингом; `Consumable`/`consume`/`on_exit` низводятся
до **сахара** над shielded outcome-defer; **одна** точка ре-диспатча unwind.

**3.1. `defer` (без изменений)** — безусловный cleanup на ЛЮБОМ exit (normal/return/throw/panic/interrupt),
кроме `exit()`. LIFO. Без cancel-shield (дёшево).

**3.2. `defer(o ScopeOutcome) { … }` (НОВОЕ — Idea B владельца)** — outcome-несущий block-defer; тело
получает `Success | Failure(any) | Panic(str)`. Субсумирует обе ретракнутые формы:
- `errdefer{…}` ≡ `defer(o){ match o { Failure(_) | Panic(_) => … } }`
- `okdefer{…}` ≡ `defer(o){ match o { Success => … } }`
- `defer |result| {…}` (D189) ≡ эта форма с типизированным биндингом.
Codegen тривиально на существующем defer-frame (`emit_c.rs:17613+`): setjmp уже знает исход (`==0`→Success,
иначе `error_kind==PANIC`→Panic, иначе Failure), `ScopeOutcome*` строится как в consume (`emit_c.rs:19290-19325`).
Bare `defer(o)` — **unshielded** по умолчанию.

**3.3. `consume X = e { body }` → САХАР** над shielded outcome-defer:
`{ ro X = e; defer(o) shielded { X.on_exit(o) }; body }`. `Consumable[E]` остаётся как protocol-сахар
(тип инкапсулирует свой cleanup); `shielded` (cancel-shield + exit-timeout, D188 R3/R4) — то, что `consume`
добавляет над bare `defer(o)`. Снимает хрупкий parser-lookahead `consume X = e {` vs `consume X = e` (D180/D196 form-4 «partial»).

**3.4. Централизованный ре-диспатч (структурно чинит баг #1)** — ОДИН runtime-helper
`nova_scope_exit(frame, outcome_kind)` (или явная общая ветка), вызываемый КАЖДЫМ setjmp-кадром
(defer, consume-сахар, with-Fail): PANIC → `nv_panic`; CANCEL → `nova_throw_cancel_reason`;
USER/USER_TYPED → handler-recoverable (иначе `nova_rethrow_with_suppressed`). Класс «один кадр забыл kind»
исчезает по построению.

**3.5. Сохранение D194 hot-path:** `Consumable[Never]` + без `WithExitTimeout` сейчас элидит shield/timeout/outcome
(disasm-verified, T2.9). После лоуэринга элизия пере-ключается на признак «**unshielded + cleanup effect-row = `Fail[Never]`** → прямой вызов без кадра». Acceptance: disasm Mutex/Sem/atomic ≡ до рефактора.

**Fallback MODEL 2** (если Model 1 — слишком большой шаг разом): un-retract ТОЛЬКО `defer(o ScopeOutcome)`
как замену errdefer/okdefer (consume-codegen не трогаем), удалить мёртвые ветки. Это **строгое подмножество**
Model 1 (без переделки при последующем апгрейде). Решение Model 1 vs 2 — в Ф.0 (sign-off).

---

## 4. Фазы

> **«сейчас»** = Ф.0, Ф.1, Ф.2 (обязательны в этом цикле). **«позже»** = Ф.3, Ф.4, Ф.5 (обязательны для
> production-grade, но другим циклом; не урезаются). Каждая фаза: задачи → spec/D/Q/docs → pos+neg тесты → критерии.

### Ф.0 — Дизайн + sign-off владельца (СЕЙЧАС, gate)
- Зафиксировать Model 1 vs Model 2 (рекомендация — Model 1); финализировать синтаксис `defer(o ScopeOutcome)`,
  правило shield-default, форму `nova_scope_exit`, пере-ключение D194-элизии.
- Закрыть открытые вопросы (см. §6) решениями владельца.
- **Выход:** утверждённый дизайн-раздел (этот файл) + новый D-блок «D314 Unified cleanup: defer-kernel».
- **Acceptance:** владелец подтвердил Model + синтаксис; §6 не содержит OPEN.

### Ф.1 — Soundness + hygiene (СЕЙЧАС, обязательно; design-risk ноль)
Багфиксы, не зависящие от выбора Model:
1. **#1 fix:** `with Fail` ре-throw'ит PANIC (ветка перед CANCEL/USER, `emit_c.rs:6646+`); сразу ввести общий
   helper `nova_scope_exit`/общую ветку (предшественник Ф.2.4). Завершает `[M-172-with-fail-swallows-panic]`.
2. **#3 fix:** удалить `?` Fail-context throw-mode (`emit_c.rs:21264-21327`) → `[E_TRY_IN_FAIL_FN]`; `?` строго
   return-only. Завершает codegen-часть [Plan 174](174-question-mark-return-only.md).
3. **#2 fix:** диагностика `D133` — quick-fix на `defer`/`on_exit` вместо `errdefer` (`types/mod.rs:15306-15318`).
4. **#4 fix:** удалить мёртвый errdefer/okdefer/defer|result| surface (AST/lexer/DeferKind/codegen-ветки);
   оставить лишь tombstone-распознавание для D189-hint. Завершает `[M-172-errdefer-okdefer-dead-surface]`.
- **spec/docs:** пометить `## D4` RETRACTED; README-индекс fix; обновить хаб.
- **Тесты pos:** `nova_tests/err173_f1/`: panic сквозь `with Fail` → процесс падает с `panic:` (fn-main +
  `EXPECT_RUNTIME_PANIC`); `?` return-only на Result/Option PASS. **neg:** `?` в Fail-fn → `[E_TRY_IN_FAIL_FN]`;
  `errdefer{}`/`okdefer{}` → `[D189-removed-*]`; (когда будет Ф.5) D133-quickfix-snapshot не содержит errdefer.
- **Acceptance:** баг #1/#2/#3/#4 закрыты; полный `nova test` зелёный; **disasm hot-path (Mutex/Sem) не деградировал**.

### Ф.2 — Унификация defer-kernel (СЕЙЧАС после Ф.0)
1. parser+AST: `defer(o ScopeOutcome) { … }` (биндинг + тело).
2. codegen: outcome-defer на defer-frame; материализация `ScopeOutcome*` и в success-ветке.
3. `consume`/`on_exit` → desugar в `defer(o) shielded { X.on_exit(o) }`; `Consumable` = protocol-сахар.
4. **#1/#4 финал:** весь ре-диспатч через единый `nova_scope_exit`.
5. D194-элизия пере-ключена (unshielded + `Fail[Never]`); disasm-парность.
6. **Multi-binding `consume`** (подтверждено 2026-06-20): `consume a = e1, b = e2, c, (x,y) = e3 { body }`
   ≡ **ВЛОЖЕННЫЕ consume-блоки** (a ⊃ b ⊃ adopt-c ⊃ (x,y) ⊃ body). `= e` — обычный биндинг; **bare `c`** —
   adopt уже-связанного `consume c` в cleanup ЭТОГО блока (c — живой consume-binding, консумится здесь, после
   блока недоступен); **tuple `(x,y) = e3`** — destructure (Plan 136 tuple-assign) + `@cleanup` на каждый
   consume-типизированный элемент. LIFO-cleanup + partial-init (D188 R1) — бесплатно из вложенности.
   Десугар: каждый биндинг → `ro X = e; defer(o) shielded { X.@cleanup(o) }` по порядку.
   - **Тесты:** multi-binding LIFO; partial-init (e2 бросил → чистится только a); bare-c adopt; tuple consume.
- **spec/D/Q/docs:** D314 (defer-kernel) + amend D188 (consume = сахар), D90 (defer-family), D189 (формы
  возвращены как `defer(o)` с биндингом); Q-cleanup-semantics обновить; хаб переписать на единую модель.
- **Тесты pos:** `nova_tests/err173_f2/`: `defer(o)` Success/Failure/Panic ветки; errdefer/okdefer-эквиваленты;
  consume-as-sugar (тот же результат, что старый consume); panic-in-defer-body composition; LIFO с outcome.
  **neg:** `defer(o)` с throw/`?`/return в теле (D90 body-constraints); двойной биндинг; неexhaustive match — на усмотрение.
- **Acceptance:** старые consume/on_exit-тесты зелёные через сахар; bare `defer(o)` работает; единый re-dispatch
  (нет per-frame дублирования); disasm hot-path ≡; **без упрощений**.

### Ф.3 — Structured-concurrency error handling (ПОЗЖЕ, обязательно)
Ответ на вопрос владельца — production-grade обработка ошибок в spawn/supervised/detach/parallel-for:
1. **scope-result API**: `supervised`/`parallel for` возвращают агрегат (по образцу Java `Joiner`):
   `all_or_throw` (любая ошибка → отмена siblings + throw MultiError), `[]Result[T,E]` (per-element, без отмены),
   `any_ok` (race-семантика). Узнать «какой fiber» — через индексированный результат.
2. **supervisor-стратегии** (sign-off 2026-06-20 — реализовать): синтаксис
   `supervised(strategy: OneForOne, max_restarts: 3, period: 5.seconds(), cancel: tok) { … }` (parens, `:`-именованные;
   заменяет несогласованный спек-постфикс `} strategy = …`). **Все 3 стратегии** (sum-тип `SupervisorStrategy`, PascalCase-варианты — как `Ok`/`Some`/`Success`)**:** `OneForOne` (рестарт упавшего),
   `OneForAll` (упал один → рестарт всех), `RestForOne` (упавшего + стартовавших после него). `period` **обязателен**,
   тип `Duration` (`N.seconds()`). Erlang/OTP-семантика: ребёнок упал → рестарт по стратегии; превышение `max_restarts`
   за `period` → супервизор падает сам (эскалация вверх). Обновить parser (`parse_supervised` 9529), AST, runtime
   (restart-loop в `nova_rt`), 08-runtime.md (заменить постфикс-форму на parens).
3. **PANIC vs CANCEL precedence** в supervised (`fibers.h:1763`); panic ребёнка → к границе fiber'а/supervisor'у.
4. **detach error-policy** + enforce `Detach`-эффект (сейчас unenforced).
5. **channel error-primitive**: различить closed vs value (recv → `Result`/типизированный), сверить `select`-семантику.
6. Согласовать stale-тесты, ошибочно утверждающие «throw неперехватываем» (`supervised_errors.nv:213`, `fiber_throw.nv:110`).
7. **🔲 Открытый surface-дизайн (резолв в ЭТОЙ фазе, не v0.1-блокер):** точный синтаксис scope-result —
   keyword-форма (`parallel for … -> []Result[T,E]`) vs метод (`scope.collect()` / `supervised_all(…)`); как
   выбирать политику (`all_or_throw` / per-element / `any_ok`) — суффикс / аргумент / отдельные конструкции; форма
   «узнать какой fiber» (индекс / label). **Возможности зафиксированы (выше); ИМЕНА — TBD.**
- **spec/D/Q/docs:** D-блок «structured error propagation»; amend 06-concurrency.md (D14/D75/D50); docs/idiom/application-effect + cancel-and-cleanup.
- **Тесты pos+neg:** `nova_tests/err173_f3/`: child throw → siblings cancelled + MultiError; per-element Result;
  race any_ok; supervisor restart (если оставлен); detach policy; channel closed vs value. neg: …
- **Acceptance:** child-fail отменяет siblings + агрегирует by default (≡ Kotlin/Java); detach-эффект enforced;
  ни одна ошибка fiber'а не теряется молча; **без упрощений**.

### Ф.4 — MultiError end-to-end + типизированный ScopeOutcome (ПОЗЖЕ, обязательно)
1. **#6:** материализовать `NovaErrorChain` → Nova `MultiError` в точке получения composed-ошибки (handler-arm/catch/scope-result); `.primary()/.suppressed()/.walk()/.find_first_panic()` работают на реальных данных.
2. **#5:** `ScopeOutcome.Failure(any)` — протянуть `error_user_payload`/`type_id`; типизированный `if err is T` в `on_exit` (D188 §typed-dispatch); `core.nv:147` → `Failure(any)`.
3. **#7:** инвариант suppressed-chain: убрать безусловный `error_suppressed=NULL` в `nova_throw`/cancel/typed ИЛИ маршрутизировать все cleanup-throw через `nova_rethrow_with_suppressed`.
4. typed-предикатный доступ (`err is CancelError`) вместо bootstrap str-prefix `"cancel: "`.
5. **🔲 Открытый surface-дизайн:** имена аксессоров `MultiError` (`.primary()`/`.suppressed()`/`.walk()`/`.find_first_panic()` — предв.) и форма typed-доступа (`err is T` vs `.downcast[T]()`); TBD при реализации.

**ФУНДАМЕНТ typed-errors / `is` — переиспользуем Plan 61, НЕ изобретаем (исследование 2026-06-20):**
Типизированная диспетчеризация **уже работает** для `with Fail[E]`:
- каждый тип ошибки → compile-time `NOVA_TID_<E>` (`type_id_registry`, USER_BASE=17, `emit_c.rs:1139`);
- typed-throw несёт `(payload, tid)` в fail-frame (`error_user_payload`/`error_user_type_id`,
  `effects.h:799-800`, `nova_throw_typed`);
- рантайм зовёт БЛИЖАЙШИЙ Fail-handler; **матчинг типа — в arm'е обработчика**:
  `if (tid == NOVA_TID_E) { e = (E*)payload; BODY } else { re-throw наверх }` (Plan 61 fu#4 —
  `fail_e_map` / `per_e_fail_types`, `emit_c.rs:1155`). Не наш тип → проходит к внешнему `Fail[Другой]`.
- → **`is T` / `try_as[T]()` = ТА ЖЕ проверка** `type_id == NOVA_TID_T`.

**Представление `any` (sign-off 2026-06-20):** fat-pointer `{ void* data; const NovaTypeInfo* vt; }`
(как Rust `dyn` / Go interface), `vt` = `NovaTypeInfo` (`type_id` + `Display`-thunk [+ `Eq`/`Hash` по
надобности]); `is`/`try_as` через `vt`-identity / `type_id`; `data` heap-boxed (GC-scanned); codegen
генерит `NovaTypeInfo` на каждый боксируемый тип. **Сосуществует с мономорфизацией** (mono — когда тип
статичен + скрытые vtable-параметры, `vtables.h:27`; fat-pointer `any` — когда тип стёрт в рантайме).
Текущее состояние: per-E dispatch работает для **user-типов** (Plan 61 fu#4); примитивы (`throw 42`) —
erased-путь (`NOVA_TID_nova_int`); Ф.3 расширяет per-E mono'd dispatch.
→ **Вывод: Ф.4 строит typed-errors (`Failure(any)` + `is`/`try_as` + `Failure(CancelError{reason:any})`)
на ГОТОВОМ type_id-фундаменте, а не с нуля.** Главный риск Ф.4 — `any`-boxing+vtable + materialization, не сам матчинг.
- **spec/D/Q/docs:** D158/D193 завершить (materialization); D190 (`ScopeOutcome[E]` остаётся rejected — type-erased); хаб.
- **Тесты pos+neg:** `nova_tests/err173_f4/`: primary+suppressed видны в handler; typed Failure dispatch; cleanup-fail во время body-fail → MultiError (не overwrite); chain переживает голый throw в unwind.
- **Acceptance:** D158/D193 обещание выполнено end-to-end; cleanup-ошибка НИКОГДА не перезаписывает body-ошибку; **без упрощений**.

### Ф.5 — Spec/D/Q/docs hygiene + exactly-once + exit-timeout (вместе с Ф.1-Ф.4, обязательно)
1. **#8:** реализовать D188 R2 exactly-once runtime-счётчик `_consume_count` + `D188-on-exit-double-invocation` (production-grade: защита от ручного/FFI double-invoke) — НЕ амендить-в-структурное (это было бы упрощение).
2. **#10:** exit_timeout 3-level (Level 1 `WithExitTimeout` vtable + Level 2 Application + Level 3 default) в едином `nv_resolve_exit_timeout_ms`; `CleanupTimeoutError` наблюдаем (в chain).
3. **#9:** `nv_resume_panic` → `nv_panic` в D188/D197 (или ввести реальный primitive).
4. **#11:** sweep stale-спеки: `## D4` RETRACTED; D90 §errdefer / D160 / D161 → historical с баннером «see D314/D188/D189»; D162-таблица; README-индекс; **хаб [error-and-cleanup-model.md](../idiom/error-and-cleanup-model.md) переписать под единую модель** (сейчас — карта 8 частично-противоречивых D-блоков).
5. **Нейминг (sign-off 2026-06-20):** (a) протокол `Consumable[E]` → `Cleanup[E]`, метод `@on_exit` → `@cleanup` — везде (`std/prelude/protocols.nv`, codegen, consume-тесты, доки, amend D188/D194); часть Ф.2 (consume = сахар). (b) эффект D185 `Cleanup` → **`ResourceTrace`**; операции `on_scope_enter`/`on_scope_exit` → **`on_resource_enter(label)`** / **`on_resource_exit(label, outcome)`**; семантика **per-resource** (N событий на N-binding `consume`-блок: enter при захвате ресурса / exit при его cleanup, LIFO); **`timeout` убрать из enter** (маргинален, per-scope-концепт — факт превышения приходит через exit-outcome `CleanupTimeoutError`). Затронуть: parser + codegen-dispatch, 3 теста `plan110/cleanup_*`, доки, amend D185. **Эффект НЕ ретрактится** (рабочий, 3/3 PASS) — переименован + resource-центричная сигнатура.
6. **`nova_runtime_reset()`** между panic-тестами в одном процессе — инфра для Ф.6 panics-clause (re-entry hazard: висящий `_nova_fail_top`/handler-iframe + uncollectable-состояние между N паниками); естественно рядом с `nova_scope_exit`.
- **Тесты pos+neg:** `nova_tests/err173_f5/`: double on_exit → `D188-on-exit-double-invocation`; exit-timeout 3 уровня; cleanup-timeout наблюдаем.
- **Acceptance:** спека выводит ТЕКУЩУЮ модель без реверс-инжиниринга кода; exactly-once и exit-timeout реальны (не заглушки); **без упрощений**.

### Ф.6 — `panics`-клаузула: panic-тесты в folder-module (−78 CU; бывш. Plan 173, ПОЗЖЕ)
Контекстное KW `panics` (инверсия PASS/FAIL): `test "…" panics "паттерн" { … }` — PASS, если тело
запаниковало сообщением ⊇ паттерн (`nv_panic` уже ловится test-frame `setjmp`, `emit_c.rs:17218-17253`).
Складывает ~114 runtime-panic тестов (36 папок) в folder-module → **−78 CU** (цель [169.1.2](169.1.2-consolidate-tests.md)).
**Гейт:** Ф.1 (panic не глотается) + Ф.5 п.6 (`nova_runtime_reset` между паниками) — без них N паник в одном
бинаре небезопасны.
- parser+AST: `TestDecl { …, panics: Option<String> }` (контекстное KW, как `raw`/`bench`).
- codegen (`emit_c.rs:17218-17253`): при `panics.is_some()` инвертировать setjmp-ветки + `strstr(msg, pattern)`; exit=0 при успехе.
- миграция: `fn main` + `// EXPECT_RUNTIME_PANIC <pat>` → `test "<stem>" panics "<pat>"`. `EXPECT_RUNTIME_PANIC` остаётся для legacy + селектора `--panic` ([169.1.1](169.1.1-test-lane-flags-and-ci.md)).
- граница: ТОЛЬКО runtime-panic (НЕ compile-error/timeout/exit — остаются `fn main`/`neg/`); конвенция: runtime-panic = `test "…" panics "…"` в folder-module (не `fn main`, не `neg/`).
- **Тесты pos+neg** `nova_tests/err173_f6/`: ожидаемая паника → PASS; неверный паттерн → FAIL; нет паники → FAIL; **N паник в одной folder-module не ломают рантайм** (Ф.5-reset).
- **Acceptance:** 114 panic-тестов в folder-module зелёные; **−78 CU**; рантайм стабилен после N паник; **без упрощений**. Маркер: `[M-173-panics-clause]`.

---

## 5. Сквозные критерии приёмки

1. **«Без упрощений, как для прода»** (ОБЯЗАТЕЛЬНЫЙ) — никаких заглушек/TODO в закрываемой функциональности;
   каждый дефект §1 закрыт реально (не задокументирован-как-известный).
2. Полный `nova test` зелёный после каждой фазы (pos+neg); новые фазовые тест-папки добавлены.
3. **Disasm hot-path** (`Mutex`/`Semaphore`/atomic, `Consumable[Never]`) не деградировал (парность с baseline до рефактора).
4. Планка «не хуже Go/Rust/TS/Kotlin/Java» (§2): все «риски» закрыты, все «выигрыши» сохранены (regression-guard тесты).
5. Спека/D/Q/docs синхронны с кодом; хаб описывает ЕДИНУЮ модель; нет stale-противоречий (D4/errdefer).
6. `panic` неперехватываем `with Fail`/`?`/handler'ом (D13), но ЗАПУСКАЕТ cleanup (defer/on_exit) — оба инварианта под тестами (pos: cleanup сработал; neg/runtime-panic: процесс умер).
7. Каждая фаза — отдельный коммит (или серия per-task); sync в main после фазы.

---

## 6. Открытые вопросы — ЗАКРЫТЫ (решения; финал — sign-off Ф.0)

| Вопрос | Решение |
|---|---|
| Унификация: одна модель или две поверхности | **Model 1 «defer — ядро» — ✅ ЗАФИКСИРОВАНО (sign-off 2026-06-20).** `consume`/`Consumable` = сахар над `defer(o) shielded`. (Model 2 более не рассматривается.) |
| Idea A (`on_exit ⇒ defer`) vs Idea B (`defer(outcome)`) | **Idea B как основа** (строго выразительнее, чисто re-абсорбирует errdefer/okdefer); Idea A реализуется как следствие (on_exit = сахар). |
| shield/timeout default для `defer(o)` | bare `defer(o)` — **unshielded**; shield+timeout добавляет ТОЛЬКО `consume`-сахар (D188 R3/R4 default-on для ресурсов). |
| D194 `Consumable[Never]` hot-path | сохранить; элизия пере-ключается на «unshielded + cleanup `Fail[Never]`»; disasm-парность — критерий приёмки. |
| `?` + auto-`From` | **отклонён** (см. [174](174-question-mark-return-only.md)); explicit `.map_err`. Cleanup-движок рассуждает только о throw+cancel+panic, не о value-`?`. |
| `ScopeOutcome.Failure` тип | **`Failure(any)`** (type-erased payload, D188); `ScopeOutcome[E]` остаётся rejected (D190). |
| MultiError | **материализовать end-to-end** (Ф.4), НЕ ретрактить (production-grade). |
| exactly-once (D188 R2) | **реализовать runtime-счётчик** (Ф.5), не сводить к структурному (это упрощение). |
| exit-timeout | **реализовать все 3 уровня** (Ф.5). |
| concurrency: child-fail | **по умолчанию отменять siblings + агрегировать MultiError** (≡ Kotlin/Java); `supervised`-вариант для isolate. |
| supervisor-стратегии | **✅ РЕАЛИЗОВАТЬ (sign-off 2026-06-20)**, НЕ ретрактить. Синтаксис: `supervised(strategy: OneForOne, max_restarts: 3, period: 5.seconds(), cancel: tok) { … }` — parens + `:`-именованные (расширяет реализованный `supervised(cancel:…)`); **заменяет** несогласованный спек-постфикс `} strategy = …` (08-runtime.md:168). **Закрыто:** (a) все 3 стратегии сразу — `OneForOne` / `OneForAll` / `RestForOne`; (b) `period` **обязателен**, тип `Duration` (идиом `N.seconds()`/`N.millis()`, std/time), пара `{max_restarts, period}` = OTP intensity/period. |
| detach | ввести явную error-policy + enforce `Detach`-эффект. |
| Нейминг: cleanup-протокол | **✅ sign-off 2026-06-20:** `Consumable[E]` → **`Cleanup[E]`**, метод `@on_exit` → **`@cleanup(o ScopeOutcome)`** (стиль `Hash`/`@hash`; убирает `-able`). |
| Нейминг: observability-эффект | **✅ sign-off 2026-06-20:** D185 `Cleanup` effect → **`ResourceTrace`**; операции **`on_resource_enter(label)`** / **`on_resource_exit(label, outcome)`** (НЕ `on_scope_*` — трейсится РЕСУРС; `label` = имя типа); семантика **per-resource** (событие на каждый consumed-ресурс, LIFO на exit); `timeout` **убран из enter** (маргинален, per-scope; превышение → exit-outcome `CleanupTimeoutError`). НЕ ретрактить (рабочий, 3/3 PASS) → освобождает `Cleanup` под протокол. ResourceTrace = ambient OTel/APM-трейсинг ресурсов (span на enter/exit с outcome). |
| Surface-синтаксис concurrency-API + MultiError | 🔲 **ОТКРЫТО — резолв в Ф.3/Ф.4** (не v0.1-блокер). Возможности зафиксированы (all_or_throw/per-element/any_ok; primary/suppressed/walk/find_first_panic). ИМЕНА/форма (`supervised_all`/`-> []Result`/`.suppressed()`, keyword-vs-метод, селектор политики, typed-доступ) — TBD при реализации. |

---

## 7. Исполнение фоновыми агентами (ОБЯЗАТЕЛЬНО соблюдать)

- **НИКАКОГО `git stash`** — `.git` repo-global, конкурентные worktree → stash/refs/reflog глобальны (collision/потеря). Для baseline — **temp-worktree** (`git worktree add`) ИЛИ **commit + reset** в своей ветке, НЕ stash.
- **`git add` только конкретных файлов** (никогда `-A`/`.`); перед коммитом — `git diff --cached --stat` (в индексе могут быть чужие pre-staged изменения).
- **Rate-limit устойчивость:** фоновые агенты в workflow иногда ловят серверный rate-limit и падают. Workflow auto-retry'ит transient; терминальные → `null`. Скрипты ДОЛЖНЫ `.filter(Boolean)` и продолжать на частичном результате; не зависеть от успеха каждого агента; идемпотентные шаги + чекпоинты (commit per task), чтобы можно было resume.
- **Параллельная правка файлов** разными агентами — каждый в своём worktree (`isolation: 'worktree'`) ИЛИ непересекающиеся файлы; иначе конфликт.
- **Тесты — только C-codegen** (`nova test` / `test-build`), интерпретатор не используется. Полный `nova test` ~60-90мин → дробить на батчи <10мин (Bash-таймаут потолок 10мин).
- Изменения `.rs` → пересобрать `nova-cli` release перед прогоном тестов; GC env (`NOVA_GC_INCLUDE_DIR`/`LIB_DIR`) — из main-репо.
- Коммит после каждой задачи; sync в main после фазы (bidirectional: pull main → ветка, merge ветка → main).

---

## 8. Источники для исполнителя (контекст)

**Хаб (точка входа):** [docs/idiom/error-and-cleanup-model.md](../idiom/error-and-cleanup-model.md).
**Решения/баги:** [docs/backlog-followups.md](../backlog-followups.md) (`[M-172-with-fail-swallows-panic]`,
`[M-172-errdefer-okdefer-dead-surface]`), [174](174-question-mark-return-only.md).
**Спека (авторитет):** `spec/decisions/08-runtime.md` (D13), `04-effects.md` (D85, NovaFailFrame, **stale ## D4**),
`03-syntax.md` (D90/D158/D160/D161/D188/D189/D190/D194/D196/D197), `06-concurrency.md` (D14/D50/D75).
**Код (истина):** `compiler-codegen/nova_rt/effects.h` (`nv_panic` 555-580, `NovaFailFrame` 55-64, `error_kind`,
`nova_throw`/`_cancel`/`_typed`/`rethrow_with_suppressed`, `nv_exit` 628), `compiler-codegen/src/codegen/emit_c.rs`
(defer/on_exit 17518-19428, **with-Fail re-dispatch 6646-6691**, `?`/`!!` = Try/Bang 21236-21445),
`compiler-codegen/src/parser/mod.rs` (D189-reject 9821-9856, supervised 9529), `ast/mod.rs:1842-1865` (мёртвые узлы),
`std/prelude/core.nv:147` (`ScopeOutcome`).
**ПРЕДУПРЕЖДЕНИЕ:** область противоречива — НЕ доверять одному файлу/summary; код = истина, спека местами stale (D4 vs D85). Верифицировать, а не верить.

## 9. Followup-маркер

`[M-173-error-system]` (umbrella) + `[M-173-panics-clause]` (Ф.6). Возможная декомпозиция в суб-планы при росте:
Ф.3 → 173.1 (structured-concurrency), Ф.4 → 173.2 (MultiError/typed-outcome).
