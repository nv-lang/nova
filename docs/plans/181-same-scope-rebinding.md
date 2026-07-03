# Plan 181 — Same-scope re-binding (`ro x = ...` повторно, тип может меняться)

> **Статус:** 📋 proposed 2026-07-02 (Ф.0 = owner sign-off gate)
> **D-блок (NEW):** **D347** (high-water = D346, Plan 180 serde; D333–D339 зарезервированы Plan 179 + резерв — verify в Ф.0).
> **Источник:** [research 2026-07-02](../research/2026-07-02-same-scope-rebinding.md) — эмпирика (11 проб) + 13 языков + 9 точек взаимодействия по коду.
> **Носитель:** main (компиляторная фича; координация с [172.1](172.1-unified-type-engine.md) — alpha-pass не трогает канал resolved_types, работает ДО него).
> **Очередность (граф 173-181 — [README планов §Очередность](README.md), 2026-07-03):** Волна 0 = Ф.0
> (owner sign-off R1-R7; fallback = `E_DUPLICATE_LOCAL` + фиксы B1/B3). Реализация — Волна 2+, **вне
> критических путей** (не гейтит и не гейтится 173-180): любой свободный слот агента; единственная
> координация — 172.1 (parser/checker-зона, alpha-pass до канала).

## §0 Контекст — статус-кво является дырой

Same-scope повторное объявление сегодня **принято фронтендом, отвергнуто бэкендом**:

- чекер НЕ имеет диагностики (тихо PASS, уже реализует shadowing-семантику типизации);
- codegen эмитит оба объявления под одним C-именем → clang `redefinition of 'x'` → CC-FAIL с ошибкой в `.c`, не в `.nv`;
- интерпретатор (UNSUPPORTED, D274) уже реализует rebind перезаписью слота — три компонента дают три разных поведения.

Плюс два смежных реальных бага (существуют независимо от решения по фиче):

- **B1 (false-positive D133):** `consume sb = ...; ...sb.into_str(); ro sb = 5` → ложный `[D133-not-consumed]` — `ConsumeCtx` ключует состояние по имени (`types/mod.rs:17387`), конфлейтит старый/новый биндинг. Контроль без затенения проходит.
- **B2 (звучность, тихая утечка):** `consume tx = begin(); consume tx = begin(); tx.commit()` — obligations по имени → одно `Consumed` гасит оба обязательства → первый `tx` утекает **молча**. Ровно тот класс, который D131/D133 обязаны ловить.
- **B3 (расхождение чекер↔codegen):** `ro x = 1; ro x = x + 1` — чекер типизирует RHS от старого `x`, эмитированный C читал бы неинициализированный новый (маскируется redefinition-ошибкой).

**Решение нужно в любую сторону** — либо фича (этот план), либо явный `E_DUPLICATE_LOCAL` (fallback Ф.0).

## §1 Дизайн — правила R1–R7 (→ D347)

```nova
ro input = read_line()
ro input = input.trim()           \ тот же тип — pipeline
ro input = parse_request(input)?  \ str → Request — тип сменился; str-версия НЕДОСТУПНА ниже
mut work = work                   \ «разморозка» (Rust: let mut x = x)
```

- **R1 Явность.** Rebind — только полной binding-формой `ro`/`mut`/`consume x = ...`. Голое `x = v` остаётся мутацией существующего `mut`-биндинга (грамматика D184 не меняется). Каждый rebind = **новая переменная**: свой тип, своя мутабельность; старая недоступна по имени ниже по тексту (значение живёт до конца scope для уже созданных захватов).
- **R2 Consume-звучность (hard error).** Затенение биндинга с непотреблённым consume-обязательством → **`E_REBIND_LIVE_CONSUME`** («переменная 'tx' (тип T) не consumed — затенение скрыло бы обязательство; потребите или переименуйте»). Nova-эксклюзив: Rust-футган «затенённый guard живёт до конца scope» становится compile error (guard'ы Nova = consume-типы, D174).
- **R3 Нерекурсивность.** RHS rebind'а видит **старый** биндинг: `ro x = x + 1` читает прежний `x` (чекер уже так работает; codegen чинится alpha-pass'ом). Haskell-грабли (`let x = f x` = `<<loop>>`) исключены.
- **R4 Захваты — на момент создания.** Замыкание/defer видят биндинг, который был жив в точке создания замыкания / регистрации defer (D90 §3 «eager»; env-снапшот замыканий уже даёт это). Rebind ниже по тексту НЕ меняет то, что они видят.
- **R5 Lint `W_SHADOW_UNRELATED`** (warn по умолчанию): новое значение **не использует** старое И старый биндинг ещё жив (не потреблён, не последнее использование). Pipeline `ro x = f(x)` — тихо. Подавление: `#allow(shadow)` (расширение D174-таргета на item/module — verify в Ф.4).
- **R6 Параметры.** Затенять параметр можно (`fn f(x int) { ro x = x + 1 }`) — Rust-прецедент, alpha-pass делает тривиальным.
- **R7 Cross-scope без изменений.** Блочное затенение работает сегодня и не трогается; политика линтов на него — вне scope плана.

**Связь со спекой:** D347 амендит D184 (binding-грамматика: повторное объявление легально) и обязан явно проговорить отличие от отвергнутого `:=` (D34 «shadowing-баги Go»): у `:=` проблема — *случайное* затенение из-за смешанного decl/assign-оператора и cross-scope протечек; здесь — явное `ro/mut` + same-scope + R2/R5. Cross-ref: D22 (R4), D90 §3 (R4), D131/D133/D180 (R2), D274 (interp уже соответствует).

## §2 Стратегия реализации — один alpha-renaming pass

**НЕ** патчить каждую name-keyed подсистему (emit_c side-tables, ConsumeCtx 10 map'ов, verify, lints — все ключуют по `String`-имени; это high). Вместо этого:

**Alpha-renaming pass** сразу после parse (до type-check / consume-check / codegen): второй и последующие same-scope биндинги имени `x` уникализируются в `x__s1`, `x__s2`, ...; **original-имя сохраняется в метаданных** (span→original map) для диагностик/hover. После pass'а весь остальной компилятор видит уже уникальные имена — consume-checker, замыкания, verify, field_cache почти не меняются.

Инварианты pass'а:
- уникализация только **same-scope** дублей (nested shadow не трогаем — работает);
- RHS rebind'а резолвится в **предыдущее** имя (R3);
- suffix-схема не пересекается с user-именами (verify: `__s\d+` в user-коде → уникализировать глубже) и с synthetic-именами field_cache (`_at_*`) / fresh_tmp;
- диагностики обязаны печатать original-имя (`x`, не `x__s1`).

Порядок пассов: parse → **alpha-rename** → (field_cache D217 — ПОСЛЕ, работает на уникальных именах) → check → codegen.

## §3 Фазы

### Ф.0 — Sign-off + пин статус-кво (gate) — small
1. Verify D-нумерация (D347 свободен; D333–D339 резерв 179).
2. Fixtures-пин текущей дыры (до реализации, как detect-набор):
   - `p01_same_type` / `p02_type_change` / `p04_mut_of_ro` / `p09_param_shadow` / `p10_self_ref` — сейчас CC-FAIL → после Ф.1 станут POS;
   - `p08b_consumed_then_rebind` — сейчас false-positive D133 → после Ф.2 POS;
   - `p08c_double_consume_leak` — B2: сейчас тихо (утечка) → после Ф.2 NEG `E_REBIND_LIVE_CONSUME`;
   - `p05_nested` / `p07_loop` — POS уже сейчас (guard против регрессий).
3. **Owner sign-off на R1–R7.** Fallback при отказе: Ф.1′ = `E_DUPLICATE_LOCAL` в чекере + фикс B1/B3 — и план закрывается.

### Ф.1 — Alpha-renaming pass + codegen — core (medium)
1. Новый модуль `compiler-codegen/src/alpha_rename.rs`: scope-stack walker по AST (fn-body/block/loop/match-arm/if-let scopes), same-scope дубли → `__sN`, original-map в side-channel.
2. Wire-in после parse во всех драйверах (check / build / test pipelines).
3. Диагностики: original-имя в сообщениях (минимум — E73xx assignability, D133-семейство, E_LOCAL_NOT_MUT).
4. B3 закрывается автоматически (RHS отрезолвлен в старое имя ДО уникализации нового).
5. Гейт: пины Ф.0 p01/p02/p04/p09/p10 → POS (компилируются и дают правильные значения); p05/p07 без изменений; **полный регресс 0-new-FAIL vs чистый baseline** (§7.5-дисциплина 172.1: сравнение с baseline, не raw count).

### Ф.2 — Consume-правило R2 — small/medium
1. `check_consume`: при `declare()` поверх имени с **Live/MaybeConsumed** obligation → `E_REBIND_LIVE_CONSUME` (расширение `check_obligations`-пути; после alpha-pass старый/новый биндинг — разные имена, конфликт виден структурно).
2. Фикс B1 (false-positive): obligations/states на уникальных именах перестают конфлейтиться — p08b POS.
3. B2: p08c NEG-фикстура ловит утечку.
4. Гейт: consume-сьюты (plan73/plan100/plan108) 0-new-FAIL.

### Ф.3 — defer/closures семантика R4 — medium
1. defer: снапшот `имя→уникальное-имя` окружения на момент **регистрации** в `DeferEntry`; inline-эмиссия на exit резолвит тело через снапшот (D90 §3). Фикс hoist-механизма (`hoisted_let_vars` — по уникальным именам).
2. Замыкания: verify, что env-снапшот после alpha-pass даёт захват старого биндинга; codegen-фикстура `p06_closure_captures_old` (f() == 1) как runtime-POS.
3. defer-фикстура: `defer` зарегистрирован до rebind → видит старое значение; после rebind второй defer → видит новое (LIFO-порядок обоих).
4. Гейт: defer-сьюты (plan100.4) 0-new-FAIL.

### Ф.4 — Lint R5 + полировка — small
1. `W_SHADOW_UNRELATED`: warn, когда RHS rebind'а не упоминает старое имя И старый биндинг не потреблён/жив. Подавление `#allow(shadow)` (verify расширение D174-таргета; если item-level `#allow` не готов — module-level, marker на item-level).
2. `nova check` вывод: hint у warning'а «если это другая сущность — используйте новое имя».
3. Негатив-фикстуры: unrelated-rebind → warning; pipeline → тихо.

### Ф.5 — Спека + доки + закрытие — small
1. **D347** в `spec/decisions/03-syntax.md` (шаблон D-блока: Что/Правило/Почему/Отвергнуто/Связь): R1–R7, отличие от `:=`, таблица проб как примеры.
2. Amend-врезки: D184 (binding), D90 §3 (defer при rebind), D131/D133 (R2), D22 (R4).
3. `docs/nv-coding-style.md`: раздел «re-binding» — когда идиоматичен (pipeline/unwrap/`mut x = x`), когда нет (unrelated).
4. Обновить `spec/open-questions.md` (закрыть Q, если заведён) + README планов + simplifications.md.

## §4 Вне scope / followups

- **`[M-181-lsp-rename-symbol-table]`** — LSP rename (word-boundary scan, D297 V1) переименует оба одноимённых биндинга; **pre-existing долг** (уже сломан для nested shadow), rebind учащает. Честный фикс = V2 symbol table. P3, home: plan-104.6 followups.
- **`[M-181-interp-parity]`** — интерпретатор UNSUPPORTED (D274); его rebind-семантика уже совпадает, но alpha-pass на interp-путь не wire'ится (нет пути). Если interp вернётся — verify.
- Cross-scope shadow-lint (Kotlin-модель warn) — отдельное решение, не здесь.
- Destructuring-rebind (`ro (a, b) = ...` повторно) — V2; в Ф.1 tuple-pattern дубли уникализируются, но R5-lint на них не распространяется.

## §5 Критерии приёмки

1. Все 11 проб research-матрицы дают спроектированное поведение (POS/NEG по таблице Ф.0), включая runtime-значения (p06 f()==1, p10 x==2).
2. B1/B2/B3 закрыты фикстурами.
3. Полный регресс: **0-new-FAIL vs чистый baseline** на nova_tests + spec_tests/conformance (один CU, 333 PASS — не задет).
4. Диагностики печатают original-имена (ни одного `__sN` в user-facing выводе — negative grep по тест-логам).
5. D347 в спеке; nv-coding-style обновлён.
