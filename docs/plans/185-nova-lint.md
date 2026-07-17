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
> - **Прогон std/**: 128 находок разобрано → 33 починены той же волной (requires на
>   take/skip, StringBuilder-концит, deque pop_back, diff reverse, servernet append-view,
>   bloom bit(), swallow-insert'ы, checked_mul/div_f64, TokenBucket acquire), остальные —
>   миграционные классы под маркерами `[M-lint-findings-*]` (backlog-followups.md) и
>   `[M-d410-as-to-migration]`/`[M-ffi-handle-newtype]`. `nova lint std/` = **0 находок**.
> - Самотест: nova_tests/lint/{conv_pos,conv_clean}.nv (4 правила pos / канон чист).
> - Семантик-апгрейды помечены `// SEMANTIC-UPGRADE:` в реестре (W_STATIC_CONVERSION,
>   W_STR_CONCAT_LOOP, W_RESULT_DISCARDED, W_IMMUTABLE_REBUILD_SETTER).
> - Из карты Ф.0 НЕ в реестре: «язык»-строки (D406-ретракция — [M-d406-retract-leading-pipe];
>   E_EXPLICIT_SELF_RETURN, `.of()`-контракт — уже в языке ✅) и `--deny`/json-вывод/
>   per-проект конфиг (Ф.3 приёмочная обвязка — следующий заход).
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

## Ф.2 — правила-линты по карте Ф.0 (каждый: правило + позитив/негатив фикстуры)

## Ф.3 — вшивание в приёмку

dev-workflow: агентская поставка обязана прогонять `nova lint --deny` по правленным
модулям; conventions-governance уже требует «вид проверки» при приёмке нового правила.

## Приёмка плана

Все строки карты Ф.0 имеют работающую проверку; `nova lint --deny std/` чист на
канонизированном std; эталонные без дельты.
