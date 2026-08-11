<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 181 — Same-scope re-binding (`ro x = ...` повторно, тип может меняться)

> **Маркер:** `[M-181-same-scope-rebinding]`. **Запуск:** «**выполни план 181**».
> **Статус:** ✅ **РЕАЛИЗОВАН (2026-07-04)** — R1–R7 + B1/B2/B3 закрыты; D347 в спеке; conformance 38/0 (d347 + amend d90/d131/d133/d22/d34); pos/neg `nova_tests/rebind/` 4/4; zero-regression vs d97c0dbe delta 0 (~135 тестов). Ф.4 R5-lint `W_SHADOW_UNRELATED` — реализован как **проектное решение**: R2 (hard-error) + demangle покрывают звучность/UX; R5 (warn) оставлен followup-маркером ниже (не гейтит корректность). Остатки: `[M-181-pattern-var-rebind]`, `[M-181-lsp-rename-symbol-table]`, `[M-181-w-shadow-unrelated-lint]` (backlog). **Ред. 2 — 2026-07-03** (аудит: D-карта, symbol-якоря, spec_tests, пины p03/p08, EXPECT_WARNING-gap, §9/§10).
>
> **Реализация (2026-07-04):** pass `compiler-codegen/src/alpha_rename.rs` (scope-walker, `x__sN` для 2-го+ same-scope биндинга, `Module.rebind_shadows` для R2); врезка ДО number_exprs/check во ВСЕХ драйверах codegen (codegen main cmd_check/cmd_compile, nova-cli cmd_build/cmd_check, test_runner) **+ bench (`nova-cli/src/bench/run.rs` run/compile_for_profile) + LSP (`nova-lsp` check_source_inner/provenance/semantic_tokens/server field-cache) — добавлены по adversarial-аудиту 2026-07-04** (были пропущены: bench давал `redefinition` CC-FAIL на benched-rebind; LSP не фаерил R2 в IDE); R2 `E_REBIND_LIVE_CONSUME` в `types/mod.rs::check_rebind_live_consume`; demangle `__sN` в `diag::render` **по множеству синтезированных имён** (thread-local map new→original из alpha_rename, НЕ regex-зеркало — иначе валидный user-идентификатор `buf__s1` стрипался бы) + demangle врезан в lint-вывод (cmd_check/cmd_build/bench/nova-codegen bin). B1/B3 закрыты уникальными именами автоматически. Ф.4 R5 `W_SHADOW_UNRELATED` → `[M-181-w-shadow-unrelated-lint]` (P3, backlog): R2+demangle дают звучность и чистый UX; шумный warn отложен по решению (Go-урок «too noisy for default»).
> **D-блок (NEW):** **D347**. **D-карта (Ред.2 2026-07-03):** committed high-water 3xx = D355 (D354/D355 в спеке);
> резервы: 178=D357–D362 · 179=D333–D337 (+D338–D339 буфер) · 180=D340–D346 · **181=D347** · 173=D348–D349 ·
> 174=D350–D353+D356 · 172.1=D400+. D347 свободен (grep=0; cross-подтверждён 173:232) — verify в Ф.0.
> **Источник:** [research 2026-07-02](../dev/research/2026-07-02-same-scope-rebinding.md) — эмпирика (11 проб) + 13 языков + 9 точек взаимодействия по коду. Сводка прецедента: **Rust/OCaml/F#/Elixir = да; Erlang/Swift/Kotlin/Java/C#/TS/Zig = нет**; уроки №1-5 (Rust guard-футган → R2; Haskell `<<loop>>` → R3; Go `:=` → отличие D34) — research §2.
> **Носитель:** main (компиляторная фича; координация с [172.1](172.1-unified-type-engine.md) — alpha-pass не трогает канал resolved_types, работает ДО него; §9).
> **Очередность (граф 173-181 — [README планов §Очередность](README.md), 2026-07-03):** Волна 0 = Ф.0-остаток
> (verify + пины; **sign-off R1–R7 ✅ 2026-07-03** — fallback `E_DUPLICATE_LOCAL` не понадобился). Реализация — Волна 2+, **вне
> критических путей** (не гейтит и не гейтится 173-180): любой свободный слот агента; единственная
> координация — 172.1 (parser/checker-зона, alpha-pass до канала).

## §0 Контекст — статус-кво является дырой

Same-scope повторное объявление сегодня **принято фронтендом, отвергнуто бэкендом**:

- чекер НЕ имеет диагностики (тихо PASS, уже реализует shadowing-семантику типизации);
- codegen эмитит оба объявления под одним C-именем → clang `redefinition of 'x'` → CC-FAIL с ошибкой в `.c`, не в `.nv`;
- интерпретатор (UNSUPPORTED, D274) уже реализует rebind перезаписью слота — три компонента дают три разных поведения.

Плюс два смежных реальных бага (существуют независимо от решения по фиче):

- **B1 (false-positive D133):** `consume sb = ...; ...sb.into_str(); ro sb = 5` → ложный `[D133-not-consumed]` — `ConsumeCtx` ключует состояние по имени (символ `struct ConsumeCtx`, `compiler-codegen/src/types/mod.rs` ~:19506, states-map ~:19512 — **снимок 2026-07-03, файл растёт ~26.7k строк: искать по символу, не по номеру**), конфлейтит старый/новый биндинг. Контроль без затенения проходит.
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

- **R1 Явность.** Rebind — только полной binding-формой `ro`/`mut`/`consume x = ...`. Голое `x = v` остаётся мутацией существующего `mut`-биндинга (грамматика D184-binding не меняется). Каждый rebind = **новая переменная**: свой тип, своя мутабельность; старая недоступна по имени ниже по тексту (значение живёт до конца scope для уже созданных захватов).
- **R2 Consume-звучность (hard error).** Затенение биндинга с непотреблённым consume-обязательством **в ТОМ ЖЕ scope** → **`E_REBIND_LIVE_CONSUME`** («переменная 'tx' (тип T) не consumed — затенение скрыло бы обязательство; потребите или переименуйте»). Nova-эксклюзив: Rust-футган «затенённый guard живёт до конца scope» становится compile error (guard'ы Nova = consume-типы, D174). **Граница:** nested-scope блок-затенение того же класса (R7 не уникализирует cross-scope → `rebind_shadows` пуст) R2 НЕ ловит — pre-existing gap consume-чекера, `[M-consume-nested-scope-shadow-leak]` (§5).
- **R3 Нерекурсивность.** RHS rebind'а видит **старый** биндинг: `ro x = x + 1` читает прежний `x` (чекер уже так работает; codegen чинится alpha-pass'ом). Haskell-грабли (`let x = f x` = `<<loop>>`) исключены.
- **R4 Захваты — на момент создания.** Замыкание/defer видят биндинг, который был жив в точке создания замыкания / регистрации defer (D90 §3 «eager»; env-снапшот замыканий уже даёт это). Rebind ниже по тексту НЕ меняет то, что они видят.
- **R5 Lint `W_SHADOW_UNRELATED`** (warn по умолчанию): новое значение **не использует** старое И старый биндинг ещё жив (не потреблён, не последнее использование). Pipeline `ro x = f(x)` — тихо. Подавление: `#allow(shadow)` (расширение D174-таргета на item/module — verify в Ф.4).
- **R6 Параметры.** Затенять параметр можно (`fn f(x int) { ro x = x + 1 }`) — Rust-прецедент, alpha-pass делает тривиальным.
- **R7 Cross-scope без изменений.** Блочное затенение работает сегодня и не трогается; политика линтов на него — вне scope плана.

**Связь со спекой:** D347 амендит **D184-binding** (binding-грамматика, Plan 114 keyword refresh — `03-syntax.md:6958`; ✅ **D184-дубль РАЗРЕШЁН 2026-07-03**: operator-dispatch-D184 → D363 (renumber); D184 = ТОЛЬКО binding (Plan 114). Ссылаться «D184-binding») и обязан явно проговорить отличие от отвергнутого `:=` (D34 «shadowing-баги Go»): у `:=` проблема — *случайное* затенение из-за смешанного decl/assign-оператора и cross-scope протечек; здесь — явное `ro/mut` + same-scope + R2/R5. Cross-ref: D22 (R4), D90 §3 (R4), D131/D133/D180 (R2), D274 (interp уже соответствует).

## §2 Стратегия реализации — один alpha-renaming pass

**НЕ** патчить каждую name-keyed подсистему (emit_c side-tables, ConsumeCtx ~10 map'ов, verify, lints — все ключуют по `String`-имени; это high). Вместо этого:

**Alpha-renaming pass** сразу после parse (до type-check / consume-check / codegen): второй и последующие same-scope биндинги имени `x` уникализируются в `x__s1`, `x__s2`, ...; **original-имя сохраняется в метаданных** (span→original map) для диагностик/hover. После pass'а весь остальной компилятор видит уже уникальные имена — consume-checker, замыкания, verify, field_cache почти не меняются.

Инварианты pass'а:
- уникализация только **same-scope** дублей (nested shadow не трогаем — работает);
- RHS rebind'а резолвится в **предыдущее** имя (R3);
- suffix-схема не пересекается с user-именами (verify: `__s\d+` в user-коде → уникализировать глубже) и с synthetic-именами field_cache (`_at_*`) / fresh_tmp;
- диагностики обязаны печатать original-имя (`x`, не `x__s1`).

Порядок пассов: parse → **alpha-rename** → (field_cache D217 — ПОСЛЕ, работает на уникальных именах) → check → codegen. **Verify-пункт Ф.1 (координация 172.1):** точка врезки согласована с M0-порядком пассов 172.1 — заявленное «ДО канала resolved_types» превращается в проверяемый пункт гейта Ф.1.

## §3 Фазы

### Ф.0 — Sign-off + пин статус-кво (gate) — small
1. Verify D-нумерация: D347 свободен (grep «## D347» + inline по spec/) и не задевает резервы D348–D349 (173) / D350+ (174); (D184-дубль РАЗРЕШЁН 2026-07-03 — operator→D363; §1).
2. Fixtures-пин текущей дыры (до реализации, как detect-набор) — **все 11 проб** research-матрицы:
   - `p01_same_type` / `p02_type_change` / **`p03_ro_of_mut`** (mut x=1; ro x=2 — симметрия p04) / `p04_mut_of_ro` / `p09_param_shadow` / `p10_self_ref` — сейчас CC-FAIL → после Ф.1 станут POS;
   - **`p06_closure_captures_old`** — CC-FAIL-пин снять уже в Ф.0 (runtime-POS f()==1 — гейт Ф.3);
   - **`p08_live_consume`** — сейчас D133-FAIL → после Ф.2 диагностика **МЕНЯЕТСЯ** на `E_REBIND_LIVE_CONSUME` (без пина смена пройдёт незамеченной);
   - `p08b_consumed_then_rebind` — сейчас false-positive D133 → после Ф.2 POS;
   - `p08c_double_consume_leak` — B2: сейчас тихо (утечка) → после Ф.2 NEG `E_REBIND_LIVE_CONSUME`;
   - `p05_nested` / `p07_loop` — POS уже сейчас (guard против регрессий).
3. **Owner sign-off на R1–R7 — ✅ ПОЛУЧЕН 2026-07-03** (по рекомендации Ред.2: фича, не запрет — alpha-pass дёшев и попутно чинит B1/B2/B3). Fallback (`E_DUPLICATE_LOCAL`) не понадобился; Ф.0-остаток = пп.1-2.

### Ф.1 — Alpha-renaming pass + codegen — core (medium)
1. Новый модуль `compiler-codegen/src/alpha_rename.rs`: scope-stack walker по AST (fn-body/block/loop/match-arm/if-let scopes), same-scope дубли → `__sN`, original-map в side-channel. (Peer-прецедент структуры pass'а — `field_cache.rs` там же.)
2. Wire-in после parse во всех драйверах (check / build / test pipelines).
3. Диагностики: original-имя в сообщениях (минимум — E73xx assignability, D133-семейство, E_LOCAL_NOT_MUT).
4. B3 закрывается автоматически (RHS отрезолвлен в старое имя ДО уникализации нового).
5. **Гейт (Ред.2-формула):** пины Ф.0 p01-p04/p09/p10 → POS (компилируются и дают правильные значения); p05/p07 без изменений; **spec_tests/conformance зелёный** + **nova_tests baseline-delta = 0** (baseline = parent-коммит, ТОТ ЖЕ бинарь, temp-worktree/commit+reset — §10; nova_tests сам по себе НЕ гейт; флака ≠ регрессия); verify-пункт врезки vs 172.1 M0 (§2).

### Ф.2 — Consume-правило R2 — small/medium
1. `check_consume`: при `declare()` поверх имени с **Live/MaybeConsumed** obligation → `E_REBIND_LIVE_CONSUME` (расширение `check_obligations`-пути; после alpha-pass старый/новый биндинг — разные имена, конфликт виден структурно). ⚠ Зона `types/mod.rs` = активная зона 172.1 — не пересекать коммиты (§9).
2. Фикс B1 (false-positive): obligations/states на уникальных именах перестают конфлейтиться — p08b POS.
3. B2: p08c NEG-фикстура ловит утечку; p08 — диагностика меняется D133→`E_REBIND_LIVE_CONSUME` (пин Ф.0 фиксирует).
4. Гейт: consume-сьюты (plan73/plan100/plan108) 0-new-FAIL (targeted per-fix канон) + формула Ф.1.

### Ф.3 — defer/closures семантика R4 — medium
1. defer: снапшот `имя→уникальное-имя` окружения на момент **регистрации** в `DeferEntry`; inline-эмиссия на exit резолвит тело через снапшот (D90 §3). Фикс hoist-механизма (`hoisted_let_vars` — по уникальным именам).
2. Замыкания: verify, что env-снапшот после alpha-pass даёт захват старого биндинга; codegen-фикстура `p06_closure_captures_old` (f() == 1) как runtime-POS.
3. defer-фикстура: `defer` зарегистрирован до rebind → видит старое значение; после rebind второй defer → видит новое (LIFO-порядок обоих).
4. Гейт: defer-сьюты (plan100.4) 0-new-FAIL + формула Ф.1.

### Ф.4 — Lint R5 + полировка — small
1. `W_SHADOW_UNRELATED`: warn, когда RHS rebind'а не упоминает старое имя И старый биндинг не потреблён/жив. Подавление `#allow(shadow)` (verify расширение D174-таргета; если item-level `#allow` не готов — module-level, marker на item-level).
2. `nova check` вывод: hint у warning'а «если это другая сущность — используйте новое имя».
3. **Проверяемость warn-фикстур (Ред.2-gap):** у раннера НЕТ маркера `EXPECT_WARNING` (только COMPILE_ERROR/RUNTIME_PANIC/TIMEOUT/EXIT_CODE/STDOUT/STDERR — test-conventions). Явный под-пункт: **либо** добавить маркер `EXPECT_WARNING <substr>` в test_runner.rs (предпочтительно — переиспользуем в других lint-планах), **либо** CLI-интеграционный тест на stderr `nova check` (fallback). Решение зафиксировать в Ф.4-коммите.
4. Негатив-фикстуры: unrelated-rebind → warning; pipeline → тихо.

### Ф.5 — Спека + доки + закрытие — small
1. **D347** в `spec/decisions/03-syntax.md` (шаблон D-блока: Что/Правило/Почему/Отвергнуто/Связь): R1–R7, отличие от `:=`, таблица проб как примеры. **D-блок самодостаточен:** «Почему/Отвергнуто» несёт компактную выжимку уроков 13 языков (Rust guard-футган → R2; Haskell нерекурсивность → R3; Go `:=` → отличие; Erlang/Elixir pin/rebind — спектр) — спека не может нормативно ссылаться на research.
2. Amend-врезки: **D184-binding** (03-syntax.md:6958, НЕ operator-dispatch-D184), D90 §3 (defer при rebind), D131/D133 (R2), D22 (R4) + резолв дубля D184 в индексе.
3. `docs/dev/nv-coding-style.md`: раздел «re-binding» — когда идиоматичен (pipeline/unwrap/`mut x = x`), когда нет (unrelated).
4. Обновить `spec/open-questions.ru.md` (rebind-Q не заведён — grep пуст; если появится к моменту исполнения — закрыть) + README планов + simplifications.md.

## §4 Тесты (Ред.2-раскладка)

- **nova_tests/rebind/** — folder-module `module nova_tests.rebind` (тема, НЕ per-plan папка): POS = peer-файлы с обычными test-блоками, runtime-asserts (p06 `f()==1`, p10 `x==2`) — обычные test-блоки, `rt/` не нужен; `_slow` не требуется.
- **nova_tests/rebind/neg/** — ТОЛЬКО compile-error: standalone `module neg.<name>`, маркер `// EXPECT_COMPILE_ERROR <substr>` **без двоеточия**, один маркер/файл (`E_REBIND_LIVE_CONSUME`: p08/p08c; `W_SHADOW_UNRELATED` — по решению Ф.4.3: EXPECT_WARNING-маркер либо CLI-тест).
- **spec_tests/conformance (ОБЯЗАТЕЛЬНОЕ D-покрытие):** NEW `spec_tests/conformance/d347_same_scope_rebinding.nv` (kinds R1-R7, включая param-shadow R6 и pipeline R5-тихий); **amend ⇒ ОБНОВИТЬ существующие d-файлы В ТОМ ЖЕ изменении:** `d90_defer_cleanup.nv` (R4-defer), `d133_consume_type_must_consume.nv` + `d131_consume_qualifier.nv` (R2), `d22_closures.nv` (R4-capture), `d34_pattern_bind_conditions.nv` (отличие от `:=`) — все пять существуют (verified ls). Binding-примеры класть в d347-файл — **НЕ создавать второй d184-файл** (существующий `d363_operator_dispatch_protocols.nv` = D363, ex-D184; дубль-D184 разрешён 2026-07-03). Прогон `nova test spec_tests` отдельной командой.

## §5 Вне scope / followups

- **`[M-181-lsp-rename-symbol-table]`** — LSP rename (word-boundary scan, D297 V1) переименует оба одноимённых биндинга; **pre-existing долг** (уже сломан для nested shadow), rebind учащает. Честный фикс = V2 symbol table. P3, home: plan-104.6 followups.
- **`[M-181-interp-parity]`** — интерпретатор UNSUPPORTED (D274); его rebind-семантика уже совпадает, но alpha-pass на interp-путь не wire'ится (нет пути). Если interp вернётся — verify.
- **`[M-consume-nested-scope-shadow-leak]`** — nested-scope double-consume-shadow утечка
  (`consume tx=…; { consume tx=… } tx.commit()`, и в теле if/for/match/while-let): R2 ловит
  только **same-scope** (alpha-rename по R7 не уникализирует cross-scope → `rebind_shadows`
  пуст; consume-obligations по имени → один commit гасит оба). **Pre-existing** (идентично на
  baseline d97c0dbe, независимо от Plan 181 — НЕ регрессия), заголовок «catches B2 leak»
  относится к same-scope. Территория D131/D133 (consume-checker), НЕ 181-scope. P3, backlog.
- Cross-scope shadow-lint (Kotlin-модель warn) — отдельное решение, не здесь.
- Destructuring-rebind (`ro (a, b) = ...` повторно) — V2; в Ф.1 tuple-pattern дубли уникализируются, но R5-lint на них не распространяется.

## §6 Критерии приёмки

0. **Без упрощений, как для прода** — ни одного «пока так»; несделанное — только явный followup-маркер (§5).
1. Все 11 проб research-матрицы дают спроектированное поведение (POS/NEG по таблице Ф.0, включая p03/p08), включая runtime-значения (p06 f()==1, p10 x==2).
2. B1/B2/B3 закрыты фикстурами.
3. **Гейт корректности (Ред.2):** `spec_tests/conformance` зелёный (d347 + amended d90/d131/d133/d22/d34) + pos/neg-фикстуры фазы + **nova_tests baseline-delta = 0** (baseline = parent-коммит, ТОТ ЖЕ бинарь, temp-worktree/commit+reset; conformance-tally на момент Ф.0 не задет — hardcode числа PASS не фиксируем, файл растёт).
4. Диагностики печатают original-имена (ни одного `__sN` в user-facing выводе — negative grep по тест-логам).
5. D347 в спеке (самодостаточный, с выжимкой 13 языков); D184-дубль разрулен; nv-coding-style обновлён; Ф.4.3-решение по EXPECT_WARNING зафиксировано.

## §9 Конвенции + координация

- **172.1 (unified type engine)** — Ф.1.2 (wire-in в драйверы) и особенно Ф.2 (`check_consume`/`check_obligations` в `types/mod.rs`) = **активная зона 172.1**: НЕ пересекать коммиты по `types/mod.rs`; verify-пункт Ф.1 — точка врезки alpha-pass согласована с M0-порядком пассов 172.1 («ДО канала resolved_types» — проверяемый пункт гейта). Порядок vs field_cache (D217) — §2.
- **D-резервы** — при коммите D347 не задеть D348–D349 (173) и D350+ (174); дубль D184 — резолв с нотой в индексе (§1).
- **Line-refs** — все номера строк в плане = снимок 2026-07-03; нормативные указатели — по символам (`struct ConsumeCtx`, `fn emit_lambda`); план исполняется медленнее, чем растут файлы.
- Конвенции: test-conventions Ред.2 (§4); spec/decisions D-шаблон; conventions-governance — изменения только по согласованию.

## §10 Агент-правила (обязательны при исполнении; Ред.2-канон)

- **Git:** НЕ `git stash` (shared `.git`, конкурентные worktree); **baseline = temp-worktree** (`git worktree add ../nova-181-base <parent>`) / commit+reset, **ТОТ ЖЕ бинарь**. `git add` только по именам (никогда `-A`/`.`); перед commit — `git diff --cached --stat`; **DCO `git commit -s`**; без `Co-Authored-By`; коммит на фазу; после фазы — bidirectional sync с main.
- **Идемпотентность (rate-limit):** commit-per-phase, no-amend, null-tolerant — падение агента не теряет работу.
- **Worktree:** постоянный `nova-p181` (naming nova-pNN), самозарегистрироваться первой командой; cwd дрейфует → абсолютные пути / `git -C`; env `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → main-repo; libuv-submodule скопировать без его `.git`.
- **Build:** **mtime-touch `.rs` перед `cargo build`** (фича компиляторная — stale-build риск). Правок std `.nv` план не несёт → rebuild nova-cli по обычному циклу компилятора.
- **Тесты:** `nova test` требует **ЯВНЫЙ путь**; батч-канон `nova test nova_tests/<dirs> --results-file rN.json` батчами <10 мин + хвост `--rerun-failed`; **ОТДЕЛЬНО** `nova test spec_tests`; targeted-сьюты per-fix (Ф.2: plan73/100/108; Ф.3: plan100.4), полный прогон — в конце фазы. Гейт = §6.3.
- **Не выдумывать синтаксис** — spec/decisions/ + examples/; подтверждение перед background-агентами.
