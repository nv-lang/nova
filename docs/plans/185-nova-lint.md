# План 185 — nova lint: машинные проверки конвенций

> Статус: ✅ ПОЛНАЯ ВЕРСИЯ 2026-07-09 (решение владельца 2026-07-09 отменило MVP-срез —
> реализовано сразу целиком, ветка lint-185). Итог:
> - **Реестр** `compiler-codegen/src/lints.rs::CONV_RULES` — 16 правил-единиц
>   {id W_*, summary, AST-хук | текст-хук}; универсальная суппрессия «остаток под
>   `[M-...]`-маркером на строке».
> - **`nova lint [paths]`** (nova-cli): прогон реестра без type-check/codegen; вывод
>   `файл:строка:кол: warning: текст [W_ID]`; exit 0/1; `--rule W_X,W_Y` выборочно;
>   `--list-rules`; скип `neg/`-фикстур; свой полный walker (test-discovery walk_nv
>   прячет peer-файлы). **`nova check --lint`** — те же правила поверх ТОГО ЖЕ реестра.
> - **Ф.3-хвост ЗАКРЫТ (2026-07-16, [M-185-lint-deny-gate]):** флаг `--deny`
>   (nova-cli/src/main.rs `Cmd::Lint`, `cmd_lint`). Без `--deny` находки
>   info-only — `warning:` в выводе, **exit 0** даже при находках (как
>   rustc warn-lints). `--deny` (bare) — денай ВСЕХ правил: находки печатаются
>   `error:`, **exit ≠0** при любом хите. `--deny=W_X,W_Y` — денай только
>   перечисленных id (валидация как у `--rule`); остальные находки остаются
>   `warning:`-only и не валят прогон. Требует `=` для значения
>   (`--deny=W_X`), иначе следующий токен ушёл бы в `paths`. lints.rs
>   (реестр/`LintWarning`) не тронут — severity/exit целиком в nova-cli.
>   Тест: `nova-cli/tests/lint_deny.rs` (5 интеграционных тестов против
>   собранного `nova`-бинаря: без-deny/exit-0, bare-deny/exit-1,
>   selective-deny match/no-match, clean-file). ВНЕ периметра этой волны
>   (решение владельца, не трогать без него): `.githooks/pre-commit` и
>   `docs/dev/dev-workflow.md`/CI (`.github/workflows/nova-lint.yml`) сейчас
>   вызывают голый `nova lint` и полагались на старую семантику exit 1 на
>   любой находке — начиная с этого фикса это уже НЕ хард-гейт без
>   явного `--deny`; синхронизация — отдельным решением владельца.
> - **Прогон std/**: 128 находок разобрано → 33 починены той же волной (requires на
>   take/skip, StringBuilder-концит, deque pop_back, diff reverse, servernet append-view,
>   bloom bit(), swallow-insert'ы, checked_mul/div_f64, TokenBucket acquire), остальные —
>   миграционные классы под маркерами `[M-lint-findings-*]` (backlog-followups.md) и
>   `[M-d410-as-to-migration]`/`[M-ffi-handle-newtype]`. `nova lint std/` = **0 находок**.
> - Самотест: nova_tests/lint/{conv_pos,conv_clean}.nv (4 правила pos / канон чист).
> - Семантик-апгрейды помечены `// SEMANTIC-UPGRADE:` в реестре (W_STATIC_CONVERSION,
>   W_STR_CONCAT_LOOP, W_RESULT_DISCARDED, W_IMMUTABLE_REBUILD_SETTER).
> - Из карты Ф.0 НЕ в реестре: «язык»-строки (D406-ретракция — [M-d406-retract-leading-pipe];
>   E_EXPLICIT_SELF_RETURN, `.of()`-контракт — уже в языке ✅). `--deny` реализован
>   (см. выше); json/junit-вывод и per-проект конфиг включения правил — по-прежнему
>   следующий заход, не входили в объём Ф.3-хвоста этой волны.
>
> Исходный статус: ПРИНЯТ (владелец, 2026-07-07: «конвенция не существует, пока не
> проверяется автоматически» — conventions-governance).

> **АМЕНДМЕНТ 2026-07-17 (финальное разкраснение `nova lint std` → 0, решение
> владельца):** реестр вырос до 19 правил-единиц (см. `CONV_RULES`). Прогон
> `nova lint std` вскрыл 12 остаточных находок пяти классов, закрыты той же
> волной:
> - **W_WITH_MUTATOR (5×, sync.nv)**: `with_lock`/`with_read`/`with_write`/
>   `with_permit` — ЛЕГИТИМНЫЙ mut-приёмник (scope-guard/RAII, Kotlin
>   `withLock`-прецедент), не field-copy. Правило получило структурный
>   различитель — fn-типовый параметр (замыкание) в сигнатуре гасит находку
>   (`conv_with_mutator` + `conv_type_is_closure`); задокументировано в
>   nv-coding-style.md рядом с существующим `with_`-разделом.
> - **W_MANUAL_SLICE_COPY (4×, prelude/embed.nv `EmbeddedDir.merge`)**: 2 —
>   генуинная contiguous-копия остатка после merge-drain → канон-фикс
>   `.append(a[i..a.len()])`; 2 — merge-interleave (не copy, comparison-driven
>   alternation) → false-positive, обход прецедентом d145 (индексация в
>   локаль перед `push`, рвёт синтаксический матч).
> - **W_STATIC_CONVERSION (2×, read_buffer.nv/write_buffer.nv `.from`)** и
>   **W_PARAM_NO_CONTRACT (1×, string/core.nv `is_char_boundary`)**: намеренные
>   постоянные отклонения (mono-баг блокирует rename; total-предикат по
>   D251) — не подходят под «остаток под `[M-...]`-маркером» (не backlog, не
>   временное). Новый механизм: **`// nova:allow W_CODE -- причина`** — inline
>   именованное подавление РОВНО правила РОВНО на следующей строке, причина
>   ОБЯЗАТЕЛЬНА (пустая → сама находка `E_LINT_ALLOW_NO_REASON`). Спека:
>   D428 в [spec/decisions/09-tooling.md](../../spec/decisions/09-tooling.md).
>   Реализация: `lints.rs::apply_nova_allow_suppressions` (+ 4 юнит-теста;
>   всего `lints::tests` 39/39 зелёные).
>
> **Итог: `nova lint std` = 0 находок, `nova lint spec_tests` = 0 находок**
> (оба гейта — 0 без единого «слепого» подавления: 3 находки погашены
> `nova:allow` с причиной, 2 — канон-фиксом, 5 — сужением правила, 2 — обходом
> по прецеденту d145).

> **АМЕНДМЕНТ 2026-07-17/18 (два новых стилевых линта, заказ владельца):**
> реестр вырос до 21 правила-единицы.
> - **W_NON_COMPOUND_ASSIGN**: `x = x OP e` при существующем компаунде `x
>   OP= e` — только `+=`/`-=`/`*=`/`/=` (`AssignOp` без `Mod`/битовых; парсер
>   лексирует ровно 4 compound-токена). LHS — простое место (ident/`@field`/
>   цепочка полей); Index-места намеренно исключены (компаунд по индексу в
>   кодогене — другой, непроверенный путь codegen'а, `emit_c.rs` гейтит
>   bounds-checked/struct-value/fixed-array write-ветки буквально `if *op ==
>   AssignOp::Assign`). Дедуп с W_STR_CONCAT_LOOP на пересекающихся сайтах
>   (тот же `in_loop && Add && стрингиш`). Канон — nv-coding-style §29.
> - **W_WHILE_COUNTER_FOR_RANGE**: `mut i = start; while i < end { …; i +=
>   1 }` → `for i in start..end { … }` — машинная проверка канона §10
>   (владелец уже словами зафиксировал этот принцип). Консервативные
>   критерии (любое нарушение → молчим): `mut i = start` НЕПОСРЕДСТВЕННО
>   перед `while` в том же блоке; условие строго `i < END`/`i <= END`
>   (`<=` → inclusive-range `..=`); тело без trailing-expr; инкремент —
>   ПОСЛЕДНИЙ statement тела; `i` больше нигде не присваивается (any depth,
>   over-conservative); `END` — простое место или int-литерал, не
>   мутируется в теле (голый Call/Index как `END` переоценивался бы каждую
>   итерацию `while`, но один раз в `for` — реальная разница); нет
>   `continue` где-либо в теле (over-conservative — вложенный цикл со своим
>   `continue` тоже гасит); `i` не используется после `while`; нет
>   `invariants`/`decreases` (SMT-контракты потерялись бы). Проверено на
>   реальном std-кейсе (`string_builder.nv::@pad_in_place` c/b/pos — c и b
>   подпадают, pos структурно не подпадает, между `mut pos=...` и любым
>   `while` всегда есть другой `mut`-let).
> - **Разкраснение волной**: 178 файлов std/spec_tests/examples — 72
>   while-counter + 304 compound-assign фикса (временный span-precise
>   codemod-фиксер `nova-cli/src/bin/fix_p185_style.rs`, применён и удалён —
>   прецедент migrate_plan60/65). 26 conformance-фикстур Plan123/LICM/IPA-
>   семьи (`licm_*`/`ipa_*`/`prop_licm_*`/`prop_ipa_*`/`plan123_*`/`v2_1_*`/
>   `v72_*`/`m5_*`) + `standalone/perf_contract_hot_loop_slow.nv` получили
>   `nova:allow W_WHILE_COUNTER_FOR_RANGE` с причиной — их СОБСТВЕННЫЙ
>   docstring подтверждает, что они пин'ят поведение LICM/field-cache-
>   оптимизатора (или изолируют перф контракт-оверхеда) ИМЕННО на
>   `while`-форме цикла; `for`-in идёт через iterator-protocol десугар
>   (D58) — другой codegen-путь, автопереписывание рискнуло бы тихо начать
>   тестировать не то. 16 юнит-тестов (7 compound-assign + 9 while-counter
>   pos/neg, включая continue-в-теле/i-после-цикла/reassign-в-теле/END-
>   мутируется/END-call/nested c-b-pos кейс).
>
> **Итог: `nova lint --rule W_NON_COMPOUND_ASSIGN,W_WHILE_COUNTER_FOR_RANGE
> std spec_tests examples` = 0 находок.** Спек-амендмент НЕ требовался —
> оба правила чисто стилевые (не меняют язык).

> **АМЕНДМЕНТ 2026-07-20 (два новых стилевых линта, заказ владельца):**
> реестр вырос до 24 правил-единиц (23 на момент реализации этой волны +
> `W_COERCE_EXPLICIT_REDUNDANT` добавлен параллельной волной D429/Plan 214,
> слит независимо).
> - **W_MANUAL_MIN_MAX**: `if a > b { a } else { b }` (зеркала `</>=/<=`,
>   обе ветви местами) → `a.max(b)`/`a.min(b)`; statement-форма без `else`
>   (`if x > hi { x = hi }`, включая mirrored-цель на правой стороне
>   condition) → `x = x.max(...)`/`x = x.min(...)`. Консервативно: ОБЕ
>   ветви обязаны буквально совпадать (по `conv_operand_key` —
>   ident/`@field`/цепочка полей/int-float-литерал) с операндами
>   сравнения — без типов не проверить отсутствие побочных эффектов
>   иначе, поэтому вызовы/индексации/произвольные выражения не флагуются.
> - **W_MANUAL_CLAMP**: трёхветочный `if X op1 B1 {B1} else if X op2 B2
>   {B2} else {X}` (обе синтаксические формы — `else if`-сахар и
>   буквальный вложенный `else {if...}`) → `X.clamp(lo, hi)` — байт-в-байт
>   форма реализации `@clamp` (protocols.nv/defaults.nv). НЕ покрывает
>   `.min().max()`-цепочки (альтернативная форма антипаттерна из задания)
>   — сознательно исключены: расходятся с `@clamp` на инвертированном
>   диапазоне `lo > hi` (доказано алгебраически в блок-комментарии
>   `conv_manual_clamp_check`), а реальных сайтов такой цепочки в корпусе
>   нет — сужение критериев без потери охвата.
> - **Само-ссылочный гейт**: оба правила молчат внутри fn с ИМЕНЕМ
>   буквально `min`/`max`/`clamp` (по имени, не по receiver'у) — `@min`/
>   `@max`/`@clamp` сами реализованы РОВНО этими if/else-паттернами
>   (`std/runtime/defaults.nv`, `std/prelude/protocols.nv` Ints-бланкет,
>   `std/time/duration/core.nv`); гасит и свободные функции-реализации
>   (`spec_tests/conformance/standalone/f11_corpus_06_pattern_regression.nv
>   ::max` — пиновая регрессия ИМЕННО этой формы) и тест-фикстуры,
>   пинующие if/else-шейп ради другого свойства кодогена
>   (`spec_tests/conformance/method_with_args_ok.nv::Bounded4_1 @clamp` —
>   тест ro-field-caching, докстринг «4 reads ro fields — cache emitted»).
> - **Дедуп между правилами**: по ТОЧНОМУ совпадению span'а ВНУТРЕННЕГО
>   if-узла (не containment внешнего span'а clamp-паттерна — открытие
>   волны: `parser::parse_if` считает span if-выражения как
>   `start(if)..end(then-блока)`, `else`-цепочка в него НЕ входит, значит
>   span внешнего узла физически НЕ содержит span вложенного). Внутренний
>   `if x > hi {hi} else {x}` трёхветочного clamp-паттерна сам по себе
>   валидный 2-операндный min/max-шейп — без дедупа получил бы ДВЕ
>   находки на одном сайте.
> - **Не-числовые Comparable-типы**: специального бланкета `@max`/`@min`
>   для произвольного Comparable-типа НЕТ (`@max`/`@min` — per-concrete-
>   type в `defaults.nv` + сам тип может определить собственный, как
>   `Duration`) — критерии линта НЕ сужались/расширялись под это (реальные
>   23 находки волны — все на числовых операндах, вопрос не всплыл на
>   практике); если такой тип встретится позже — отдельное решение.
> - **Разкраснение волной**: 23 находки (std/spec_tests/examples), все
>   исправлены на `.max()`/`.min()`/`.clamp()` (`std/collections/vec/
>   views.nv::@first_n`/`@last_n` — два ПОСЛЕДОВАТЕЛЬНЫХ `if` объединены
>   в один `.clamp(0, @len)`, семантически идентично при `0 <= @len`,
>   всегда истинно для длины `Vec`). 25 юнит-тестов (обе формы × оба
>   правила, pos/neg/self-ref/дедуп).
>
> **Итог: `nova lint --rule W_MANUAL_MIN_MAX,W_MANUAL_CLAMP std
> spec_tests examples` = 0 находок.** Спек-амендмент НЕ требовался — оба
> правила чисто стилевые (не меняют язык).

## Цель

Каждое принятое конвенционное правило получает машинную проверку: предупреждение чекера
(W_*), greppable-инвариант с точной командой в тексте конвенции, либо запрет на уровне
языка. Прогон — частью `nova test` (std-поверхность) и отдельной командой `nova lint`.

## Решение владельца 2026-07-09: MVP-срез — SUPERSEDED

> **SUPERSEDED тем же днём:** позднейшее решение владельца 2026-07-09 отменило
> двухэтапность — реализована сразу полная версия (см. статус выше). Врезка
> сохранена для истории.

Реализация в два этапа (лимит-экономия):
1. **MVP (сейчас, САМЫЙ минимальный — уточнение владельца):** правила как
   отдельный реестр-модуль внутри чекера, активируются флагом `nova check --lint`;
   правил — 3-5 образцовых (максимальный улов при тривиальной реализации:
   W_NONVARIADIC_OF, W_RETIRED_PREFIX, W_FFI_BARE_HANDLE + по месту), они же
   задают форму единицы-правила для всех последующих.
   Реестр проектируется как ФИНАЛЬНАЯ архитектура: правило = самостоятельная
   единица (id W_*, walker-хук, диагностика), никакой привязки к check-пайплайну
   сверх точки вызова.
2. **Полный `nova lint` (потом):** тонкая сабкоманда nova-cli поверх ТОГО ЖЕ
   реестра + exit-коды, json/junit-вывод, per-проект конфиг включения; остальные
   правила карты доезжают haiku-батчами по готовому образцу. Переписывания
   правил при переходе НЕТ.

## Ф.0 — свод правил → вид проверки (карта)

| Правило (дом) | Проверка | Вид |
|---|---|---|
| суммы только `type X enum` (D406) | ретракция leading-` \| ` в парсере ([M-d406-retract-leading-pipe]) | язык |
| явный `return @` в `-> @` (D409) | `E_EXPLICIT_SELF_RETURN` | язык ✅ |
| пустой `.of()` (D259-амендмент) | контракт `requires args.len() > 0` | язык ✅ |
| `Vec[` вне vec-модуля (D238/D239-амендмент) | `W_VEC_SPELLING` | линт |
| Голый int/*() хендл в extern-семействе с new/open+free/close (module-conventions §4а, 2026-07-09) | `W_FFI_BARE_HANDLE` | линт |
| `.of` у невариадика (§21б nv-coding-style) | `W_NONVARIADIC_OF` | линт |
| Статик-конверсия T.from(x)/T.parse(s) (§1а) | `W_STATIC_CONVERSION` | линт |
| Поэлементная копия `push(x[i])` в счётном цикле (§18а nv-coding-style) | `W_MANUAL_SLICE_COPY` | линт-эвристика |
| Метод без `mut` возвращает Self с пересборкой всех полей (D117/D409, OpenOptions-класс) | `W_IMMUTABLE_REBUILD_SETTER` | линт-эвристика |
| `as_`-префикс (D410) | `W_RETIRED_PREFIX` (as_) | линт |
| `get_`/`set_` пары (D117 AMEND) | `W_ACCESSOR_PAIR` | линт |
| мутирующий `with_*` (nv-coding-style §21) | эвристика: `with_*` с `mut @`-приёмником → `W_WITH_MUTATOR` | линт |
| `try_` без инфаллибельного сиблинга (R3 D325) | `W_TRY_WITHOUT_SIBLING` | линт |
| сеттер не `-> @` (D117 AMEND-2) | 1-арный метод-свойство `mut @x(v)` с `-> ()` → `W_SETTER_NOT_FLUENT` | линт |
| `buf = buf + x` в цикле (perf-conventions) | `W_STR_CONCAT_LOOP` | линт |
| тихое глотание `Result` (стиль §4: swallow-match, `ro _ =`, отброшенный statement) | `W_RESULT_DISCARDED` | линт |
| index/offset/len-параметр без `requires` (стиль §5) | `W_PARAM_NO_CONTRACT` (public std) | линт |
| `Fail[` в публичной std-сигнатуре собственных ошибок (R5 D325) | conformance-guard (есть) + `throw`-скан | греп |
| `nth`/`to_bytes`/`to_chars`/`.into()`/`with_capacity`/`from_raw_parts` (ретракции) | греп-инварианты `= 0` (команды в D-блоках) | греп |

## Ф.1 — инфраструктура

`nova lint [PATH]`: прогон W_*-правил без кодогена (checker-pass); флаг `--deny` (W→E) для
CI/приёмки агентских поставок. Линты живут в чекере (types/mod.rs), правила — таблицей
(не хардкод по месту, §3 compiler-conventions).

**✅ `--deny` реализован (2026-07-16, [M-185-lint-deny-gate])** — см. итог в статусе
плана выше.

## Ф.2 — правила-линты по карте Ф.0 (каждый: правило + позитив/негатив фикстуры)

## Ф.3 — вшивание в приёмку

dev-workflow: агентская поставка обязана прогонять `nova lint --deny` по правленным
модулям; conventions-governance уже требует «вид проверки» при приёмке нового правила.

> Механизм `--deny` готов (см. Ф.1). Само вписывание в
> `docs/dev/dev-workflow.md`/`.githooks/pre-commit`/CI (`.github/workflows/nova-lint.yml`,
> сейчас зовут голый `nova lint` и полагаются на старую семантику «любая находка =
> exit 1») — ВОПРОС владельцу, не сделано в этой волне намеренно (узкий периметр
> задачи 2026-07-16: только флаг+тест+этот документ).

## Приёмка плана

Все строки карты Ф.0 имеют работающую проверку; `nova lint --deny std/` — **не чист**:
`--deny` вскрыл 3 pre-existing находки `W_PARAM_NO_CONTRACT` (std/collections/{hashmap,
queue,set}.nv, параметр `cap` конструктора `new` без `requires`) — дрейф корпуса ПОСЛЕ
lint-sanitation-волны 2026-07-10, не связан с реализацией `--deny` (то же самое видел бы
и старый `nova lint std` — он ловил ЛЮБУЮ находку как exit 1 ещё до этого фикса).
Вне периметра узкой задачи 2026-07-16 (--deny механизм); зафиксировать отдельным
заходом (contract-волна §5, см. `[M-lint-findings-param-no-contract]` в
backlog-followups.md — смежный, но не идентичный класс находок).
