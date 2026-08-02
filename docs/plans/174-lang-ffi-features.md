# Plan 174 — Language & FFI features on the unified type engine (umbrella)

> **Статус:** 📋 READY (umbrella). Создан 2026-06-27; **Ред. 2 — 2026-07-03**: полная сверка семейства
> (9-агентный аудит-workflow: 6 × под-планы, зонт/статусы, 7-языковая планка, конвенции). Зонт превращён
> из индекса в **launch-документ**: порядок/приоритеты, Ф.0R-реконсиляция под-планов, планка
> Rust/Go/TS/Kotlin/Java/Zig/Swift с обязательными дополнениями, тест-раскладка, D-номера, агент-правила.
> **Маркер:** `[M-174-lang-ffi]`. **Запуск:** «**выполни план 174**» (разворачивается в §7a).
> **Carrier:** Plan 172.1 (unified type engine) — но связка у под-планов РАЗНАЯ (§2; blanket-гейт
> «ждать MVP 172.1» снят Ред. 2 — веха U.1-U.4 заморожена, носитель живёт в режиме D-трека).
>
> **Очередность (граф 173-176 — [README планов §Очередность](README.md), 2026-07-03):** Волна 0 = Ф.0R;
> Волна 1 трек A = **174.3 (🔴 P1 — критический путь: гейтит 173 Ф.4)** ∥ 174.4 ∥ 174.6-M0. **Входящие:**
> 174.2-остаток ← 173 Ф.1; 174.6 M1-M3 ← M0; 174.1/174.5 ← D-трек 172.1 (Волна 3). **Исходящие:** 174.3 →
> 173 Ф.4; 174.6 §2 (CWStr) → 176 Ф.2.
> **ОБЯЗАТЕЛЬНЫЙ сквозной критерий приёмки: «без упрощений, как для прода»** — для зонта и КАЖДОГО
> 174.x (в т.ч. 174.1/174.2, где формула отсутствовала). Фазирование = порядок, не урезание объёма.

## 0. Происхождение

Изначально фичи были самостоятельными планами **171 / 174 / 175 / 176 / 177 / 178**. Аудит 2026-06-19
свернул их в зонт 172 (172.6-172.11, коммит `69d3e5e5`); решением владельца (2026-06-27) вынесены в зонт
174 — зонт 172 сфокусирован на ядре движка (172.1-172.5). Forward-compat «172.1 не форклоузит 174.x» —
[172-compiler-rework.md](172-compiler-rework.md) §3.1/§3.2. **NB:** от старой нумерации в под-планах
остались stale-ссылки (171/175/176/177/178 в старом значении) — вычищаются в Ф.0R (§3.0), это
**прямые ошибки запуска** (например «выполни план 176» в 174.4 отправит агента в чужой план io/fs).

## 1. Под-планы (статусы Ред. 2)

| # | План | Суть | Статус / гейт |
|---|---|---|---|
| **174.1** | [Primitive parse API](174.1-primitive-parse-api.md) | Один generic-движок str→примитив (**вариант B** — Ред. 2: type-set bounds ДОСТУПНЫ, [172.3 ✅ CLOSED 2026-06-28](172.3-type-set-bounds.md)); radix-only `parse`; фикс truncation-бага; float-канон (§4.1). D309. | 🚧 **§7.7-оценка + truncation-фикс DONE 2026-07-06** (`emit_parse_range_check` sub-width i8/i16/i32/u8/u16/u32 в обеих хардкод-ветках; 20 pos-тестов). `@parse_int→Result` **поглощён 177**. Живой остаток (generic-движок + хардкод-removal + typed-errors + float-канон + radix-поверхность) = `[M-174.1-parse-engine-structural]`, координация 172.1-hardcode × 177 |
| **174.2** | [`?` return-only](174.2-question-mark-return-only.md) | **Остаток = spec-closure + cross-carrier диагностики** (§3.2). Codegen/checker-ядро (`[E_TRY_IN_FAIL_FN]`, удаление throw-mode, баннер D4) **реализует [173 Ф.1 п.2](173-error-system-unify-harden.md)** — направление «173 завершает 174.2», НЕ наоборот. Amend D85. | ✅ **DONE 2026-07-06:** spec-closure D85 (auto-From блок, D165-ref фикс) + миграция 7+сайтов `?→!!` + Ф.B cross-carrier (`E_TRY_OPTION_IN_RESULT_FN`/`E_TRY_RESULT_IN_OPTION_FN`); conformance neg 53/0; E1≠E2-hint отложен (`[M-174.2-try-err-type-mismatch-hint]`, gate sum-extension 172.1) |
| **174.3** | [`any` + downcast](174.3-any-type-and-is-downcast.md) | `any` fat-pointer `{data, vt=NovaTypeInfo*}` + `is T`/`try_as[T]` по `type_id` (инфра Plan 61 ГОТОВА). **🔴 P1 — ПЕРВЫЙ в семье**: гейтит [173 Ф.4](173-error-system-unify-harden.md) («реализуй первым»); реализация НЕ ждёт 172.1. Amend D53/D54 (+ D351 при выделении). | 📋 PROPOSED (P1-first) |
| **174.4** | [Effect-registry size](174.4-effect-registry-compile-time-size.md) | Compile-time N вместо хардкода 32 (`effects.h:971`; silent-drop 33-го эффекта подтверждён :996-1002). **Carrier-независим** — исполним сейчас, параллельно 174.3. Без D-блока (+ обязательный Q-note). | ✅ Ф.1 DONE 2026-07-04 (`-DNOVA_MAX_EFFECT_STORAGES=N` во все TU + abort вместо silent-drop; Q-note D11; Ф.2 static-indices — follow-up) |
| **174.5** | [Pointer-ops methods](174.5-pointer-ops-methods.md) | Методы `.read/.write/.offset/.dist/…` вместо операторов; `unsafe T`→`uninit T`; write-cap fix (жив: `types/mod.rs:12093`, `emit_c.rs:26117/39336`). Amend D216-pointer (+ D352 для uninit-семантики). | 📋 PROPOSED; **§7.7-оценка DONE 2026-07-06:** write-cap-баг ЖИВ (spec §11a:8522 голый `*unsafe T` writable; checker `.write()` минует `pointee_is_writable`:13847; emit_c `:27263`/`:40347`), гейт **РЕАЛЬНЫЙ** (02-types = зона 172, «не в одиночку»). Checker/codegen/spec-amend отложены в координируемое 172-окно |
| **174.6** | [C-FFI ABI types](174.6-ffi-abi-types.md) | C-ABI тип-лист (рекурсивный) + `E_FFI_NON_C_ABI_TYPE` + fn-ptr ABI-тег `*extern "C" fn`. Amend D282-FFI (+ D353 для ABI-тега). Поглощает `[M-172.1-extern-cname-dedup-overloads]` (✅ dedup-hardening в M1). | 🚧 M0 (spec) ✅ DONE + **M1 (parser `*extern "C" fn` + checker `E_FFI_NON_C_ABI_TYPE` + коэрция-гейт + error-index + dedup) ✅ DONE 2026-07-04**; M2–M3 (cast-матрица/`_Static_assert`/cookbook) — follow-up |

## 2. Порядок, приоритеты, гейты

**Порядок исполнения (Ред. 2):**
1. **Ф.0R** (§3) — реконсиляция семейства (stale-номера, D-коллизии, направление 173↔174.2) — ДО всего.
2. **174.3** — 🔴 P1-first: критический путь 173 Ф.4. Реализация на инфре Plan 61 (`type_id_registry`
   `emit_c.rs:1241`, `fail_e_map` :1249), НЕ ждёт финала 172.1.
3. **174.4** — параллельно 174.3 (carrier-независим, READY).
4. **174.2-остаток** — после/вместе с 173 Ф.1 (ядро кода — там; Ф.B cross-carrier диагностики —
   собственная checker-часть 174.2).
5. **174.6** — M0 (spec: amend D282 rule 2 + D216 cross-amend) — исполним сейчас (renumber-гейт
   ✅ снят 2026-07-03, D282/D216 однозначны). M1-M3 (checker/тег/тесты) — после M0, не ждёт
   172.1-финала (checker-слой аддитивен).
6. **174.1, 174.5** — на носителе: оба лезут в legacy-зону, которую 172.1 D-трек переписывает
   (174.1 — хардкод `emit_c.rs:28006/28061/40131`; 174.5 — `02-types.md` «не править в одиночку»).
   **Гейт переопределён** (веха «MVP U.1-U.4» заморожена): затронутые планом D-блоки зелёные на канале
   D-трека 172.1 + координация с владельцем 172.1 перед стартом. 174.1 дополнительно: тройная
   координация 172.1 (hardcode-зона) × 177 Ф.2b (rename SHIPPED-имен) — см. §3.1.

**Межсемейные связи:** 174.2/174.3 ↔ [Plan 173](173-error-system-unify-harden.md) (Ф.1/Ф.4);
174.6 ↔ [Plan 176](176-io-fs-os.md) (главный потребитель тип-листа; NB: формулировка 176 «Windows-extern
принимает []u16» противоречит грамматике — `[]u16` = Vec = GC-тип; синхронизировать на `(*u16,len)`/CWStr).
Между собой 174.x независимы, кроме порядка выше.

## 3. Ф.0R — Реконсиляция семейства (СЕЙЧАС, перед любым 174.x; тексты правок готовы)

### 3.0 Stale-номера старой нумерации (прямые ошибки запуска)
- **174.4:4** «Запуск: „выполни план 176“» → «выполни план 174.4» (сейчас 176 = io/fs — агент уйдёт в чужой план!).
- **174.4:6** «не блокирует(ся) 175» → уточнить: старое 175 = нынешний 174.3; текущий 175 = time-system.
- **174.6:7** «Не путать с 177 (pointer-ops)» → «…с 174.5»; текущий 177 = Result-everywhere (и он в 174.1
  упоминается в НОВОМ значении — в одной семье «177» сейчас значит два разных плана).
- **174.1:141** «171 = его per-type реализация» → «174.1».
- Тест-папки со stale-номерами: `plan171`/`ptr177`/`ffi178` → §5-раскладка.

### 3.1 174.1 (parse)
- **172.3 ✅ CLOSED** → переписать на **вариант B**: `fn[T SignedInt] / fn[T UnsignedInt]
  T.try_from(s)` и `T.parse(s, radix)` с `T.MIN/T.MAX` в generic-теле (~22 сигнатуры → ~6-8;
  `SignedInt`/`UnsignedInt` уже в prelude `protocols.nv:704-705`); догма «T.MAX в generic не резолвится»
  ОПРОВЕРГНУТА showcase 172.3 §5. Per-type остаются только f64/f32/bool (+char). **Sweep ВСЕХ
  упоминаний per-type/172.3 по суб-плану**: §2, §3 (D309 «interim»), §4 (опровергнутая догма),
  §5 Ф.2 (per-type try_from) и Ф.6 («future-collapse»), §7 риск 4, §8 («desirable, НЕ blocking»).
- Ф.3: добавить **второй mirror-сайт** хардкода `emit_c.rs:40130-40151` (план знает только основной);
  **re-grep ВСЕ emit_c.rs/08-runtime.md refs суб-плана** при внесении правок (затронуты §1/§3/§4/Ф.3;
  известные пары: :27036→28006, :27091→~28061, :35113→36562, :27073→truncation-сайт рядом с 28006,
  Ф.3-списки :27036-87/:27117-78; D74 = 08-runtime.md:2143, D77 = :2333 с телом ниже; json.nv:464).
- **Внести сквозной критерий «без упрощений, как для прода» + acceptance-секцию** (в 174.1 acceptance
  есть — верифицировать формулу; в шапку добавить сквозной критерий).
- Секвенирование с 177 Ф.2b закрыть: все НОВЫЕ поверхности 174.1 создаются сразу под D325-именами
  (D325 уже в спеке 04-effects.md:6201); за 177 Ф.2b — только rename/удаление SHIPPED-триады parse.nv.
- **char:** добавить `char.try_from(s str)` (single-codepoint + `ParseCharError`) — Ф.3 вырезает
  hardcode `nova_str_to_char` (:28024; в mirror-сайте :40130-40151 char-арм = тип-маппинг :40149),
  замена обязана существовать.
- **Float-канон (§4.1-планка):** не «тонкая обёртка над strtod» — strtod скипает whitespace
  (нарушает no-trim) и locale-зависим (`LC_NUMERIC`). Решения (Ред. 2): pre-check no-trim ДО вызова;
  locale-независимость (C-locale guard / ручной парс); `f32` — напрямую strtof (НЕ f64→narrow:
  double-rounding); full-consume (`"1.5abc"` → Err); `inf`/`infinity`/`nan` case-insensitive (Rust);
  hex-float НЕ поддерживаем (маркер-followup); underscore `1_000` → Err (Rust-модель); `+`-префикс
  принимается; `"0x10"` при radix:16 → Err(InvalidDigit).
- Тест-раскладка → §5; acceptance-дополнения → §4.1.

### 3.2 174.2 (`?` return-only)
- Зафиксировать разделение: **реализация ядра = 173 Ф.1** (п.2 + его spec/docs- и тест-буллеты:
  E_TRY_IN_FAIL_FN, удаление `emit_c.rs:21895-21958`, баннер D4). **Владелец обновления
  `spec_tests/conformance/d85_question_return.nv` (+ `neg/d85_try_in_fail_fn_neg.nv`) = 173 Ф.1 —
  в одном изменении с кодом** (тест закрепляет поведение, которое ставит 173); 174.2 Ф.A трогает
  d85-файл ТОЛЬКО если amend-текст D85 идёт отдельным изменением после. Остаток 174.2:
  **Ф.A spec-closure** (переписать тело D85: return-only канон + отклонение авто-`From` в блоке, не
  NB-врезке; НЕ ссылаться на «D165» как D-блок — это error-code семейства Plan 100), миграция
  aspirational-примеров (`examples/effect_density/http.nv`, `examples/real_world/oxsar_port.nv`,
  doc-comment `@commit()?`→`!!` — 4 сайта: 03-syntax.md:8345, core.nv:140, protocols.nv:445,
  errors.nv:271 — плюс 3 docs-сайта: cleanup-cookbook.md:68, idiom/consume-scope-cleanup.md:62,
  idiom/consume-types.md:40 — **итого 7 сайтов**; 3 docs-сайта передаются в чеклист 173 Ф.2 п.10
  doc-sweep с добавлением `?→!!` к его rename-скоупу, spec/prelude-сайты — 174.2 Ф.A);
  **Ф.B cross-carrier диагностики**
  (собственная checker-часть, carrier-независима): `?` на Option в `->Result`-fn → ошибка с hint
  `.ok_or(...)`; Result-`?` в `->Option`-fn → hint `.ok()`; `Result[T,E1]`-`?` при `E1≠E2` → hint
  `.map_err`; пины: `a? ?? b` прецеденс, `foo()?.bar()` = postfix-then-member (НЕ Kotlin safe-call),
  `?` в lambda пробрасывает из lambda; **`?` в main — РЕШЕНО (Ред. 2): main может возвращать
  `Result` (Rust-модель) → `?` легален; в unit-main → та же carrier-mismatch диагностика**.
- Добавить acceptance-секцию (отсутствует!) + «без упрощений»; тесты → §5 (pos в `effects/`,
  neg в `effects/neg/`; **обновить существующий `spec_tests/conformance/d85_question_return.nv`
  В ТОМ ЖЕ изменении** + `neg/d85_try_in_fail_fn_neg.nv`).

### 3.3 174.3 (`any`)
Дозакрыть 4 однострочных решения (Ред. 2): **(a)** канон проверки = `type_id` (не vt-identity);
**(b)** boxing-таблица по ABI-классам: pointer-ABI → `data` = сам `Nova_X*` (без double-box, Go-style);
value-ABI (NovaValue_X, nova_str, примитивы, туплы) → heap-box копия; таблицу — в amend D53;
**(c)** мост для 173 Ф.4: `nova_typeinfo_from_tid()` (генерённый switch, симметрично
`nova_typeid_to_name` из typeid.c) — пункт Ф.1; **(d)** MVP NovaTypeInfo = `{type_id, name, size,
Display}`, append-only ABI; Eq/Hash/Clone-thunks = Ф.3. Плюс: **(e)** список позиций implicit
upcast T→any (параметр/присваивание/return/`[]any`-литерал) — в amend D53; **(f)** `is` с generic-T
(`x is Vec[int]`) — MVP ЗАПРЕТ с внятной диагностикой (снятие — Ф.3); **(g)** `==`/Hash на `any` до
Ф.3 → **compile-error** «no Eq/Hash on any» (НЕ тихий pointer-identity; лучше Go, который тут
runtime-паникует); **(h)** TID-стабильность: whole-program sequential ordinal — Q-блок в
08-runtime.md (валиден single-CU; dylib/multi-CU → name-hash) + требование детерминизма присвоения
между пересборками (byte-identical baseline); **(i)** пины: `42 as any is int`=true / `is i64`=false
(разные TID); `Option[*T] as any` roundtrip (NPO-edge); flow-narrowing при mut-переприсвоении —
правило в D54-amend (Kotlin-прецедент: запрет smart-cast на мутируемое). §4: +amend D26/prelude
(где объявлены `try_as`/`as` в .nv — nv-sourcing); §8: line-refs освежить (1139→1241, 1155→1249,
21540→22182; 03-syntax 3166→3178, 3427→3443). Явный TBD «Eq/Hash/Clone-thunks» закрыт пунктом (d).

### 3.4 174.4 (effect-registry)
Предрешения (Ред. 2): **(a)** N==0 → clamp `max(N,1)` (элизия = отдельная оптимизация Ф.2);
**(b)** Q-note в spec/decisions/08-runtime.md — ОБЯЗАТЕЛЕН (наблюдаемая семантика наследования
handler-стека); **(c)** guard переполнения — НЕ `<assert.h>`-assert (NDEBUG стирает → в release
вернётся silent-drop!): собственный `fprintf(stderr, имя эффекта+capacity)+abort()`, живущий и в
release, включая fallback-путь (#ifndef 32); **(d)** **детерминизм**: N и порядок регистраций/индексов
— сортировка по qualified effect name (HashMap-итерация сломает byte-identical baseline); **(e)** план
закрывается по Ф.1; Ф.2 (статические индексы) — под маркером `[M-174.4-effect-registry-size]`;
**(f)** оговорка N-overshoot (effect_schemas ⊇ реально регистрируемых storage — безопасно).
Line-refs: emit_user_effect_registrations = :17882; `_nova_register_all_effects_` = :17780-17833;
snapshot ~:7877-8039, 8698. Зонт-исключение: 174.4 НЕ зависит от 172.1 (см. §1).

### 3.5 174.5 (pointer-ops)
- **D138 → D238/D240** (Index/MutIndex; D369 = межпакетный импорт, ex-D138 renumber 2026-07-03!) — §1.1, §3.
- Stale-ссылка `02-types.md:8278` → §11a write-таблица (:8498-8499) — во всех ~5 местах (4× полная
  форма :18/:102/:140/:149 + голое «8278» :138 — grep по паттерну `8278`); вообще ссылаться
  §-именами (файл в зоне 172, строки дрейфуют). NB write-cap: `:26117` — гейт по const-префиксу,
  `:39336` — mirror-инференс write БЕЗ const-проверки (часть дефекта).
- **Добавить Ф.0 rename-sweep** `unsafe T`→`uninit T` (без неё Ф.1 неисполнима — её acceptance уже в
  uninit-терминах): parser (`uninit` = contextual keyword в type-position + blast-radius grep
  идентификаторов), AST `Unsafe(T)`→`Uninit(T)`, sweep ~97 вхождений в 5 spec-файлах (02-types.md ~90,
  04-effects.md 4, 03-syntax.md, README.md, open-questions.md — финальный объём по grep `unsafe T`) +
  D246 §V3.2 flip-таблица + D218-retraction кросс-рефы + docs/guide/typed-pointers.md + E-сообщения; амендмент §V2.3
  (flow-sensitive read после definite-assignment — меняет «read requires unsafe always»).
- **wrapping_offset — противоречие закрыто (Ред. 2): deferred** (`[M-174.5-wrapping-offset-deferred]`
  остаётся) — вычистить из Ф.2-acceptance и §8-тестов.
- **Flow-sensitive uninit (M2/M3/S3) — отдельная Ф.5** с acceptance и тестами (pos flow-narrowing read;
  neg E_UNINIT_VALUE_MOVE; definite-assignment if/else/цикл/поля; `ro p uninit T` → ошибка;
  init-при-объявлении → ошибка) — сейчас крупная фича без фазы.
- Acceptance для Ф.3 (все retired-формы → E_POINTER_OP_USE_METHOD с fix-it, ДО @-method/@index
  фоллбэка) и Ф.4 (blast-radius отчёт; 0 сайтов операторов в std/nova_tests; полный регресс).
- Контракты в amend D216 (п.d): **provenance & GC** (Boehm: адрес-как-int на стеке/GC-куче пиннит;
  спрятанный в C-malloc/xor — НЕ скан; `p as int as *T` roundtrip легален) + **alignment per-метод**
  (.read/.write UB при unaligned — потому есть `_unaligned`) + **.dist контракт** (same allocation,
  разница кратна sizeof — Rust offset_from) + **.offset UB-границы** (переполнение n*sizeof) +
  volatile-scope (примитивы; struct — задокументировать неатомарность). FFI-null: декрет «FFI
  возвращает `Option[*T]` через NPO» + фикстура None-из-NULL.
- Мелочи: `dst as *mut u8` в S1 (не `*u8`); `.copy_to(dst *mut T, n int)` = memmove-зеркало;
  `.write` возврат unit→`*mut T` = behavior-change (в blast-radius Ф.2); миграция
  plan118_5_v3_t9 строк 19-20 И 23-24; заявить закрытие `[M-118.4-typed-ro-write-error]`,
  `[M-118.4-struct-ptr-read]`, `[M-118-ptr-arithmetic]`, `[M-118.1-volatile-ops]`; cross-lang
  таблица (Zig `[*]T`-контрпример отработать: у Zig арифметика только на many-item типе — у Nova
  различие single/many НЕ в типах, гейт = unsafe; Swift/Go/Java — метод/функция-стиль ЗА план).

### 3.6 174.6 (FFI ABI)
- **Добавить секцию фаз**: M0 = amend D282 rule 2 + D216 cross-amend (spec-first; «после M1» для
  rule 3 → определить M1 явно); M1 = рекурсивный C_ABI-классификатор в checker (detect non-fatal →
  счёт по std/nova_tests → enable `E_FFI_NON_C_ABI_TYPE`); M2 = rule 3 `*extern "C" fn` (тип,
  коэрция iff captureless ∧ C-ABI ∧ **пустой эффект-лист**, cast-матрица); M3 = тесты + error-index
  + ffi-cookbook.
- **NPO-набор расширить**: `Option[X]` C-ABI iff X ∈ {`*T`, `*()`/CStr, fn-ptr, newtype-over-`*T`}
  (= D216 §7 NPO-eligible; текущее правило «только *T» уже, чем реальность — ffi.nv:12-13 возвращает
  nullable newtype). `Option[*extern "C" fn]` — nullable C-callback.
- **D216 §10 ретракции — явным списком** в §4: (a) строка «*fn = default C ABI» ретрактится
  (*fn = Nova-ABI); (b) `E_CALLBACK_THROWS_OVER_C_ABI` переезжает на коэрцию → `*extern "C" fn`;
  (c) новые строки cast-матрицы §12 (implicit `*fn`→`*extern "C" fn` ЗАПРЕЩЁН — разные ABI-теги =
  разные типы; явный as-cast в unsafe); (d) `*extern "C" unsafe fn` — существует (композиция §10a);
  (e) `[M-118-stdcall-fn-ptr]` — тег «C» = platform default cdecl, stdcall отдельно.
- **Callbacks × фиберы/GC (планка, крупное)**: (i) коэрцируемая в `*extern "C" fn` функция — БЕЗ
  эффектов (пустой лист; neg-тест fn с Fail → ошибка); (ii) entry-guard/trampoline: регистрация
  C-треда в GC (`GC_register_my_thread`) + init effect-TLS; pos-тест: C-тред зовёт Nova-callback,
  который аллоцирует → нет краха; (iii) yield/spawn в callback — бан.
- **Ownership/pinning**: декрет «Nova-указатели через FFI — borrowed на время вызова; retain →
  pinning-API (`GC_malloc_uncollectable`/keep-alive registry)» — в D282 + ffi-cookbook (Boehm не
  сканирует C-malloc память → сохранённый C-кодом указатель = use-after-free).
- **Layout — S8** (**ПЕРЕОЦЕНКА 2026-07-04, 174.6 M2/M3 — НЕ закрыт сейчас, обоснованный defer**):
  замысел был эмитить в `.c` `_Static_assert(sizeof/offsetof)` для каждого value-record/тупла через
  FFI-границу (аналог repr(C)). При реализации выяснилось: корректный `<expected>` = C-ABI размер
  структуры С паддингом/выравниванием — независимая layout-модель, которой у Nova нет (полагается на
  C-`sizeof`). `sizeof==sum-полей` **неверно** (отвергает легальный паддинг `{i8,int}`=16≠9);
  `sizeof(NovaValue_X)==sizeof(NovaValue_X)` **тавтология** (для СВОЕЙ эмитированной структуры C считает тот
  же размер — реальный S8-дрейф возможен только против ВНЕШНЕЙ C-либы, чей layout Nova не знает).
  newtype-over-`*()`→nova_int erasure (net.h:253-259) — это **platform-инвариант** (`sizeof(void*)==sizeof(nova_int)`),
  не per-user-value-record. Итог: осмысленный per-type static-assert **coupled** к отложенной полной
  layout-спеке → закрывается ВМЕСТЕ с ней (`[M-174.6-ffi-struct-layout]`), а тавтологичный/неверный guard
  = запрещённое упрощение. Детальное обоснование — 174.6 §11.
- Прочее: fixed-array поля `[N]u8` — добавить в C_ABI грамматику (D228 «fully inline» уже требует);
  char в Scalar-листе — оговорка (uint32_t с инвариантом валидности; Rust improper_ctypes флагает);
  str = `{ptr,len}` НЕ NUL-terminated — в cookbook; varargs → маркер `[M-174.6-varargs]`;
  «(S5)»/«(S8)» — inline-расшифровать; ссылка «08-runtime.md:8285» → :8155; поглотить
  `[M-172.1-extern-cname-dedup-overloads]` (backlog OPEN-view :763-772, назначен 174.6) явным пунктом.
- spec_tests: имя `d282_*` уже занято другой темой (blanket protocols) → `d282_ffi_abi.nv`
  (дизамбиг до renumber-решения §6).

### 3.7 Кросс-файловые мелочи
- `172.2-method-arg-type-checking.md:6` — подпись «D309=171» → «D309=174.1»; `wip/172.1-d-status.md:410`
  (строка D309; НЕ :411 — там D314/173) — из ярлыка убрать «Method arg narrowing», оставить
  «primitive parse API» (сейчас содержит оба).
- ~~Plan 173:353 stale-строки (1139/1155 → 1241/1249)~~ — ✅ поправлено 2026-07-03 (Ред. 2 этого зонта).
- README.md строка 174.1 «impl-coupled к 172.3» → пометить ✅.
- Беклог: зарегистрировать в OPEN-view (`docs/plans/backlog-followups.md`) маркеры
  `[M-174-lang-ffi]` (зонт) и отсутствующий у 174.1 маркер `[M-174.1-parse-api]`.
- **Acceptance Ф.0R:** все правки §3.0-§3.7 внесены; grep = 0 по: `план 176`, `ptr177`, `ffi178`,
  `plan171`, `q_return_only`, `D138 Index`, `8278` (в 174.5), `D165` (как D-блок, в 174.2);
  **полные 7-языковые таблицы внесены в каждый под-план** (§4.1-§4.6; 4.4 = N/A-строка);
  сквозной критерий «без упрощений» стоит во ВСЕХ шести под-планах (вкл. 174.1/174.2);
  **без упрощений**.

## 4. Планка «не хуже Rust/Go/TS/Kotlin/Java/Zig/Swift» (Ред. 2 — ОБЯЗАТЕЛЬНЫЕ дополнения)

Сводка по фичам (полные таблицы — в под-планы при Ф.0R; здесь нормативные требования и acceptance):

**4.1 parse (174.1).** Nova уже выше Java/Kotlin/TS/Swift (типизированная ошибка + Result;
InvalidRadix вместо Rust-паники) и на уровне Rust/Go/Zig по int. Обязательные дополнения (см. §3.1
float-канон): acceptance-фикстуры `+42`→Ok; `1_000`→Err; `0x10`@radix16→Err; radix 1/37→Err(InvalidRadix),
2/36→Ok; границы i64::MAX±1, u64-max, `-0` на uint→Err, `007`→Ok(7); float: `' 1.5'`→Err (no-trim ДО
strtod), `1.5abc`→Err, `1e999`→Ok(+inf) пин, inf/NaN case-пин, locale-независимость = отдельный
критерий, f32 double-rounding пин; bool `True`/`TRUE`/`1`→Err.

**4.2 `?` (174.2).** Решение Nova (return-only, без скрытого From) чище Rust. Обязательное: cross-carrier
диагностики с fix-it (§3.2 Ф.B — Rust-прецедент ok_or/ok; без них ошибки будут generic), пины
прецеденса `?`/`??`, `foo()?.bar()`, lambda-граница, main.

**4.3 any (174.3).** Дизайн Go/Rust-класса; с Display-thunk — ЛУЧШЕ Rust (Any не даёт Display). Обязательное:
compile-error на `==`/Hash-any до Ф.3 (Go тут runtime-паникует — быть лучше); TID-детерминизм + Q-блок;
запрет generic-T в `is` (MVP); пины int-vs-i64, NPO-edge, mut-narrowing правило (§3.3).

**4.4 effect-registry (174.4).** Планка = Zig-принцип «нет произвольных лимитов». Обязательное:
release-guard (не NDEBUG-assert), детерминизм порядка (byte-identical), edge N=32/33/0/1.
Языковая поверхность не меняется — 7-языковое сравнение N/A (одной строкой в плане).

**4.5 pointer-ops (174.5).** Метод-набор = Rust-эталон (add/offset_from/read/write/unaligned/volatile);
Swift/Go/Java — метод/функция-стиль ЗА план; Zig-контрпример (`[*]T` даёт операторы) отработать явно.
Обязательное: provenance&GC док-секция, alignment/dist/offset контракты в D216-amend, FFI-null декрет
(§3.5); `uninit T` сверка: Rust MaybeUninit / Zig undefined — усиливает rename.

**4.6 FFI (174.6).** `E_FFI_NON_C_ABI_TYPE` hard-error = строже Rust-lint (Zig-класс); тег
`*extern "C" fn` = Swift-класс. Обязательное: callbacks×фиберы/GC (3 требования §3.6), pinning-декрет
(✅ ffi-cookbook 174.6 M3), char-оговорка (✅ ffi-cookbook тип-таблица), `_Static_assert` layout
(**defer с обоснованием** — coupled к полной layout-модели, см. §3.6-переоценку + 174.6 §11; тавтологичный
guard = упрощение). Прецеденты в план: Rust improper_ctypes /
Go cgo pointer-rules / Zig extern struct / Swift @convention(c) / Java Panama.

## 5. Тест-раскладка семейства (конвенции; выверено 2026-07-03)

**Правило:** темы-folder-module, НЕ per-план папки (test-conventions: минимизируй CU):
- 174.1 → peer-файлы `plan174_1_*.nv` в **`nova_tests/str/`** (+ `str/neg/`); НЕ `plan171/`.
- 174.2 → **`nova_tests/effects/`** (+ `effects/neg/`); НЕ `q_return_only/`.
- 174.3 → **`nova_tests/any_is/`** — новая тема ОПРАВДАНА (нет существующей); folder-module
  `module nova_tests.any_is` + `any_is/neg/`.
- 174.4 → **`nova_tests/effect_registry/`** — отдельная папка ОПРАВДАНА причиной, которую вписать в
  план: тесты «>32 эффектов» и «малый N» проверяют per-binary N ⇒ обязаны быть РАЗНЫМИ CU.
- 174.5 → **`nova_tests/pointers/`** (файлы `plan174_5_*.nv`) + `pointers/neg/`; НЕ `ptr177/`.
- 174.6 → **`nova_tests/ffi/`** (или peer в `plan91_12/`) + `ffi/neg/`; НЕ `ffi178/`.

**spec_tests/conformance — ОБЯЗАТЕЛЬНО** (сейчас нет НИ в одном 174.x): на каждый затронутый D:
174.1 — `d309_*.nv` + обновить `d74_math_methods`-семью/D77; 174.2 — **обновить существующий
`d85_question_return.nv` в том же изменении** (после amend старый тест = анти-норма) + neg;
174.3 — `d53_any_fat_pointer.nv`, `d54_any_is_downcast.nv` + neg; 174.4 — N/A (нет D; явной строкой);
174.5 — `d216_pointer_methods.nv` (D216 однозначен после renumber ✅); 174.6 — `d282_ffi_abi.nv`
(имя свободно: бывший d282_blanket переименован в `d355_blanket_protocol.nv`). Прогон: `nova test spec_tests` (один CU) + targeted-тема.

**Правки test-conventions.md (по governance, «· согласовано»; обоснование — дыры, вскрытые аудитом):**
(a) **sweep «nova test требует путь»** — конвенция рассинхронизирована с fd7a8da5: строка :481 «Без
аргументов — nova_tests/» ЛОЖЬ, ~14 bare-примеров (:469,:497,:502-503,:508,…) падают usage-error →
переписать (синхронизация с уже согласованным 172.6); (b) чеклист-шаг: «правка трогает/амендит D →
СНАЧАЛА spec_tests/conformance (новый d-файл ИЛИ обновление существующего + neg/), потом nova_tests»;
(c) фраза «amend D ⇒ существующие d<NNN>-файлы этого D обновляются в том же изменении»;
(d) dev-workflow §5.5: полпредложения «nova test требует явный путь (172.6)».

## 6. D-номера семейства (verify 2026-07-03)

- **174.1 = D309** (свободен; зарезервирован кросс-ссылками 172.2:6 и wip/172.1-d-status:410 — оставить,
  поправить stale-ярлыки §3.7). Остальные 174.x новых D пока не резервируют (амендменты D53/D54/D85/
  D216/D282) — коллизий с занятыми (D314, D327, D333-D339, D340-D346, D347, D348-D349) НЕТ.
- **Резерв семьи: D350-D356** (D350-D399 свободны, выше — серия D400+ занята 172.1): D350 = 174.2
  (если «? return-only» выделится из amend D85 в отдельный D); **D351 = 174.3** (any runtime-repr +
  NovaTypeInfo ABI — новая нормативная зона, чище отдельным D); **D352 = 174.5** (flow-sensitive
  uninit); **D353 = 174.6** (fn-ptr ABI-тег); **D354/D355 — приёмники renumber** (следующий пункт);
  D356 — резерв. Проставить в шапки под-планов («предв. DNNN; финал при impl») — Ф.0R.
- **✅ Renumber двойных D-номеров ВЫПОЛНЕН (sign-off владельца + исполнение 2026-07-03):**
  anon-tuple-mono **D216 → D354** (typed-pointers сохраняет D216 — цепочка V2/V3-амендментов,
  D246, 174.5); blanket-protocols **D282 → D355** (extern-ABI сохраняет D282 — канон README-индекса,
  амендится 174.6). Обновлены: заголовки блоков + Эволюция-ноты (02-types.md), все anchor-ссылки
  (06-concurrency/10-overloading/03-syntax/README), planы 59.1/161/162/wip/172.1-d-status/p67/checklist,
  conformance-файлы переименованы `d354_generic_anon_tuple_mono.nv` / `d355_blanket_protocol.nv`
  (включая внутренние идентификаторы). Бонус: nova_tests/plan163 ссылались на D282 ошибочно →
  исправлено на D288. **Гейт 174.5/174.6-M0 снят.**

## 7. Исполнение фоновыми агентами (ОБЯЗАТЕЛЬНО; под-планы ссылаются сюда, не копипастят)

1. **НИКАКОГО `git stash`** (repo-global, конкурентные worktree). Baseline — **temp-worktree**
   (`git worktree add ../nova-174-base <commit>`) ИЛИ **commit+reset** в своей ветке, ИЛИ
   patch+checkout. Канон naming постоянного worktree: `git worktree add -b plan-174 ../nova-p174 main`.
2. **Git:** add только конкретных файлов (никогда `-A`/`.`); перед коммитом `git diff --cached --stat`;
   **DCO `git commit -s`** (CI-гейт); без Co-Authored-By; коммит per task; sync в main после фазы.
3. **Rate-limit:** workflow-агенты иногда падают на серверном rate-limit. Скрипты — `.filter(Boolean)`,
   идемпотентные шаги, чекпоинты (commit per task), `resumeFromRunId` для резюма; не зависеть от
   успеха каждого агента.
4. **Сборка:** `cargo build --release --manifest-path nova-cli/Cargo.toml` → `nova-cli/target/release/nova.exe`;
   изменил `.rs`/`nova_rt` → пересобрать до прогона; в worktree — mtime-touch `.rs` + env
   `NOVA_GC_INCLUDE_DIR`/`NOVA_GC_LIB_DIR` на main-репо + libuv-submodule копия без `.git`.
5. **Тесты:** только C-codegen; **`nova test` требует явный путь** (fd7a8da5). Per-fix — targeted
   тема; полный прогон — батчами <10 мин (`nova test nova_tests/<dir1> <dir2> … --results-file rN.json`,
   хвост `--rerun-failed`); отдельно `nova test spec_tests` и `nova test std`. Гейт корректности =
   spec_tests + detect-фикстуры; nova_tests = baseline-delta (тот же parent-коммит, temp-worktree).
6. **Параллельные правки** — worktree-изоляция или непересекающиеся файлы; `02-types.md`/`08-runtime.md`
   горячие (зона 172) — координация с владельцем 172.1, не править в одиночку.
7. **Логи:** после фазы — project-creation.txt + discussion-log.md (nova-private) + simplifications.md.

## 7a. Запуск-чеклист («выполни план 174» разворачивается в это)

1. Прочитать зонт целиком + под-план текущего шага + §8-источники под-плана.
2. **Ф.0R первым** (§3) — правки семейства, коммит per под-план.
3. Далее по §2-порядку — три ПАРАЛЛЕЛЬНЫЕ ветки старта: **174.3 (P1)** ∥ **174.4** ∥ **174.6 M0**;
   затем 174.2-остаток (синхрон с 173 Ф.1) → 174.6 M1→M2→M3 →
   174.1, 174.5 (гейт: D-трек 172.1 по затронутым D + координация владельца).
4. **Prerequisite-check на входе каждого под-плана** (стоп + эскалация при незакрытом гейте):
   174.2 ← 173 Ф.1 (кодовая часть); 174.1 ← координация 172.1×177 Ф.2b; 174.5 ← Ф.0 rename-sweep
   свой + зона 172. (Renumber-гейт D216/D282 ✅ снят 2026-07-03.)
5. Каждая задача: код → targeted тест → commit -s → лог; фаза/под-план: гейты §8 → sync main.

## 8. Критерии закрытия зонта

1. **«Без упрощений, как для прода»** — сквозной для зонта и каждого 174.x (включая дополнения §4).
2. Все 174.1-174.6 landed по СВОИМ acceptance (+ обязательные дополнения §3/§4 Ред. 2);
   Ф.0R выполнена первой.
3. spec_tests/conformance-покрытие всех затронутых D (§5) зелёное; nova_tests baseline-delta = 0.
4. Планка 7 языков: требования §4.1-4.6 закрыты тестами/декретами; Nova-превосходства
   (typed parse-errors, compile-error на any-Eq, hard-error FFI-лист, callbacks-контракт)
   зафиксированы в спеке.
5. Спека/D/Q/docs синхронны: D309 (+D350-D356 по факту выделения), амендменты D53/D54/D85/D216/D282,
   Q-noты (TID-стабильность, effect-registry), ffi-cookbook/typed-pointers.md обновлены;
   renumber-решение D216/D282 принято владельцем.
6. Маркеры закрыты/переведены: `[M-174.1-parse-api]`, `[M-174.2-question-return-only]`,
   `[M-174.3-any-is]`, `[M-174.4-effect-registry-size]` (Ф.2-остаток допустим открытым с новым home),
   `[M-174.5-pointer-ops-methods]`, `[M-174.6-ffi-abi]`, `[M-D282-ffi-abi-type-list]`,
   `[M-172.1-extern-cname-dedup-overloads]`, `[M-138.5-unsafe-ptr-write-cap]` (поглощён 174.5 Ф.1);
   заявлено закрытие legacy `[M-118-*]`-семьи (§3.5).
7. test-conventions-правки §5 внесены с «· согласовано».

## 9. Followup-маркеры

`[M-174-lang-ffi]` (зонт; регистрация в OPEN-view — Ф.0R §3.7) + per-план маркеры (§8 п.6) +
`[M-174.1-parse-api]` (создать) + отложенные: `[M-174.5-wrapping-offset-deferred]`,
`[M-174.6-ffi-struct-layout]` (**ПЕРЕОЦЕНКА 2026-07-04, 174.6 M2/M3:** static-assert-часть §3.6 НЕ
закрывается сейчас — корректный `_Static_assert(sizeof==<expected>)` требует независимой C-ABI
layout-модели [паддинг/выравнивание], которой у Nova нет; `sizeof==sum-полей` неверно [отвергает
паддинг], `sizeof==sizeof` тавтология [для СВОЕЙ структуры C даёт тот же размер; S8-дрейф — только против
ВНЕШНЕЙ C-либы]. Значит static-assert **coupled** к «полной layout-спеке», а не отделим от неё; закрытие
требует сначала layout-модели. Детальное обоснование — 174.6 §11. Остаток = полная layout-спека +
static-assert поверх неё), `[M-174.6-varargs]` (новый), hex-float parse (174.1, новый при реализации).
