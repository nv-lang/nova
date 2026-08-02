# Same-scope re-binding (`ro x = ...` повторно, тип может меняться) — исследование

> Дата: 2026-07-02
> Метод: 3 параллельных агента — (1) эмпирика на живом компиляторе (11 проб, release nova CLI, main @ 3626c83c), (2) кросс-языковой прецедент (Rust/OCaml/F#/Erlang/Elixir/Go/Swift/Kotlin/Java/C#/TS/Haskell/Zig), (3) анализ 9 точек взаимодействия по коду компилятора.
> Контекст: вырос из вопроса «всегда писать ro/mut при присваивании + переменную можно переобъявить с новым типом». Полная форма (source-SSA, «всегда») отклонена в обсуждении (ломает циклы/аккумуляторы/`@f = v`); исследована узкая форма — **same-scope re-binding**.
> Итог: **план [181-same-scope-rebinding.md](../../plans/181-same-scope-rebinding.md)**.

## TL;DR

1. **Статус-кво — дыра, решение нужно в любую сторону.** Чекер НЕ имеет диагностики same-scope re-binding (тихо проходит и уже реализует shadowing-семантику типизации); codegen эмитит оба объявления под одним C-именем → clang `redefinition of 'x'` → CC-FAIL с непользовательской ошибкой (указывает в `.c`). Плюс два реальных бага рядом: false-positive D133 при затенении *потреблённого* consume-биндинга и семантическое расхождение чекер↔codegen на `ro x = x + 1`.
2. **Фича обоснована** (pipeline-идиом, недостижимость stale-значения, давление в сторону `ro`), цена — **medium** при реализации через один alpha-renaming pass.
3. **Nova структурно закрывает главный Rust-футган** (затенённый guard живёт до конца scope): RAII нет, guard'ы — consume-типы → затенение непотреблённого consume = compile error (правило R2).

---

## 1. Эмпирика: что компилятор делает СЕЙЧАС

Пробы: scratchpad `rebind_probes/`, каждая отдельным файлом; `nova check` + `nova test` (C-codegen). Формат: `module rebind_probes.<file>` + nova.toml с `std`-путём.

| # | Проба | `nova check` | `nova test` (codegen+rt) | Вывод |
|---|---|---|---|---|
| 1 | `ro x=1; ro x=2` same type | **PASS, 0 warnings** | **CC-FAIL** `redefinition of 'x'` (ошибка в `.c`) | чекер разрешает, C ломается |
| 2 | `ro x=1; ro x="hello"` смена типа | PASS — assert `x=="hello"` типизировался (чекер = shadowing-семантика) | CC-FAIL `redefinition ... 'nova_str' vs 'nova_int'` | C: два decl `x` без переименования |
| 3 | `mut x=1; ro x=2` | PASS | CC-FAIL | смена мутабельности не спасает |
| 4 | `ro x=1; mut x=x+1; x=10` (идиома `let mut x = x`) | PASS | CC-FAIL | идиома не работает |
| 5 | вложенный блок `ro x=1; { ro x=2 }; x==1` | PASS | **PASS** (inner==2, outer==1) | блочное затенение корректно |
| 6 | closure+rebind: `ro x=1; ro f=fn()->int=>x; ro x=99` | PASS | CC-FAIL; в C виден снапшот `_env->x = x;` **ДО** rebind → f() вернул бы **1** | капча старого биндинга — уже правильная семантика |
| 7 | `for i in 0..3 { ro x = i }` | PASS | **PASS** | fresh-binding на итерацию работает |
| 8 | `consume sb=SB.new(); sb.append(..); ro sb=5` | **FAIL `[D133-not-consumed]`** (тип в сообщении пустой) | то же | утечка поймана — единственная Nova-диагностика |
| 8b | consume **полностью потреблён** `into_str()` ПЕРЕД `ro sb=5` | **FAIL — та же D133** | то же | **FALSE POSITIVE**: трекер конфлейтит биндинги по имени |
| 9 | `fn f(x int) { ro x = x+1 }` затенение параметра | PASS | CC-FAIL (параметр = тот же C-scope) | затенить параметр нельзя (по факту) |
| 10 | `ro x=1; ro x=x+1` self-ref RHS | PASS (RHS видит **старый** x) | CC-FAIL; C эмитил бы `nova_int x = checked_add(x,1)` — **self-init нового** | расхождение чекер↔codegen, замаскировано redefinition-ошибкой |

**Дополнительно по коду:** в `types/mod.rs` нет диагностики redeclaration для локалов (только type-name/protocol-method shadowing); интерпретатор (`interp/env.rs:37-39`, UNSUPPORTED по D274) уже реализует rebind-семантику перезаписью слота.

**Дыра звучности consume (по коду, п.3 ниже):** `consume tx = begin(); consume tx = begin(); tx.commit()` — obligations хранятся по имени → одно `Consumed` гасит оба обязательства → **первый tx утекает молча**.

---

## 2. Кросс-языковой прецедент

| Язык | Same-scope rebind | Cross-scope shadow | Тип меняется | Главный урок |
|---|---|---|---|---|
| Rust | ✅ идиоматично | ✅ | ✅ | rebind + immutable-by-default убивает временные имена; нужен ответ на drop-порядок затенённого (guard-футган) |
| OCaml | ✅ (цепочка let-in) | ✅ | ✅ | в ФП rebind = норма десятилетиями |
| F# | ✅ в функциях; ❌ module-level | ✅ | ✅ | разумно разделить: локально свободно, top-level строго |
| Erlang | ❌ (`=` = match) | ❌ | — | запрет порождает `State1/State2/State3`-класс багов |
| Elixir | ✅ | ✅ (не течёт из блоков) | ✅ | rebind в паттернах ⇒ pin-оператор `^`; забытый pin — топ-футган |
| Go | ⚠️ `:=` частично | ✅ (класс err-багов) | ❌ | гибрид хуже обоих чистых вариантов; shadow-линтер «слишком шумный для дефолта» не работает |
| Swift | ❌ | ✅ идиоматично (`if let x`) | ✅ в shadow | главный use-case (unwrap) встроен в синтаксис ветвления |
| Kotlin | ❌ | ✅ + warning | — (smart casts) | flow-narrowing снимает большую часть потребности |
| Java / C# | ❌ | ❌ для локалов | — | максимальная строгость → многословие; pattern vars как выход |
| TypeScript | ❌ (`let`) | ✅ + no-shadow lint | — (narrowing) | control-flow narrowing — сильнейшая альтернатива |
| Haskell | ❌ в одном let-блоке | ✅ (`-Wname-shadowing`) | ✅ | **let обязан быть нерекурсивным** — иначе `let x = f x` = `<<loop>>` |
| Zig | ❌ | ❌ полностью | — | «имя = одна вещь» исключает shadow-баги грамматикой, ценой нумерации имён |

### Топ-5 уроков

1. **Rebind безопасен ровно в паттерне `ro x = f(x)`** (новое из старого; clippy `shadow_reuse`). Опасен `shadow_unrelated` (старое не участвует и ещё живо) → warn по умолчанию. Rust держит все 3 линта allow-by-default — и guard-футган остаётся необнаруживаемым.
2. **Судьба затенённого значения — самый недооценённый пункт.** Rust: затенённый `MutexGuard` живёт до конца scope → удержанный лок, раздутые async-фреймы. **В Nova не применимо:** RAII нет (D188/D90 — явный cleanup), guard'ы = consume-типы → правило R2 превращает футган в compile error.
3. **Замыкания захватывают биндинг на момент создания** — один и тот же футган во всех rebind-языках; семантика правильная, но нужен явный пункт в спеке + тесты (в Nova env-снапшот уже даёт это).
4. **Pin-проблема паттернов**: Nova уже сделала выбор — паттерны всегда биндят свежие имена (D34, `E_AMBIGUOUS_IDENT_PATTERN`), сравнение через guard. Pin-оператор не нужен.
5. **Парадокс мейнстрима**: Java/C#/TS запретили *безопасное* (same-scope: старое имя недоступно) и разрешили *опасное* (cross-scope: две живые одноимённые, класс Go-`err`-багов). Nova-конфигурация — инверсия: same-scope разрешить с правилами, cross-scope оставить как есть (работает) + возможный lint позже.

Альтернатива на горизонте: **flow-typing/narrowing** (TS/Kotlin/Swift) закрывает мотив «тот же объект, уточнённый тип» без новых биндингов — но у Nova narrowing только `is`-smart-cast в if; «трансформация значения с потерей старого» (`ro s = parse(s)`) narrowing'ом не покрывается.

---

## 3. Взаимодействия с механизмами Nova (по коду)

**Общий факт:** в компиляторе нет resolution-pass'а с binding-id — AST-идент = `String`, все подсистемы ключуют локалы **по имени**. Nested shadowing работает «случайно» (overwrite в HashMap + вложенные C-блоки).

| Точка | Текущее устройство | При rebind | Стоимость |
|---|---|---|---|
| **consume (D131/D133/D180)** | `ConsumeCtx.states: HashMap<String, VarState>` (`types/mod.rs:17387`) + ~10 name-keyed map'ов; `declare()` перезаписывает state, obligations не трогает | false-positive (проба 8b) и **тихая утечка** double-consume-shadow; нужно правило R2 + разделение биндингов | alpha-rename → low-medium; правило R2 → low |
| **Замыкания (D22)** | immutable capture = **by-value снапшот** в env при создании (`emit_c.rs:33853+`); mut = указатель на C-локал | с уникализацией имён замыкание естественно держит старый биндинг — нужная семантика сама | low |
| **defer/errdefer (D90 §3)** | тело клонируется в `DeferEntry`, эмитится **inline на exit** — имена резолвятся в позиции exit'а; hoist по имени | нужен снапшот `имя→уникальное-C-имя` per DeferEntry (резолв на момент регистрации, как велит D90 §3) + фикс hoist | medium |
| **Codegen C-имён** | `pattern_binding` → сырое Nova-имя; эмиссия `"{ty} {name} = {val};"` без уникализации; десятки side-table'ов по имени | alpha-renaming pass ДО codegen → emit_c не трогается | low-medium (pass) vs high (патчить emit_c) |
| **Checker дубликаты** | НЕТ проверки повторного локала (все duplicate-чеки top-level) | вводить фичу — удалять нечего; отвергать — надо ДОБАВИТЬ `E_DUPLICATE_LOCAL` | low в обе стороны |
| **Контракты/Z3 (D24/D110)** | substitution-model: `BvScope.subst.insert(name, val)` с overwrite — **уже семантически rebind**; sort меняется свободно | SSA-подобное затенение кодированию *проще*; риск только rebind имени параметра из `ensures` — alpha-rename закрывает | low |
| **Field caching (D217)** | pass инжектит `ro _at_<F> = ...` с суффиксами `_r<N>`/`_n<N>` — сам построен в обход отсутствия rebind | не ломается; требование — порядок alpha-pass'а относительно field_cache; опционально фича упрощает pass | low |
| **LSP rename (D297)** | word-boundary regex scan, НЕ symbol resolution — переименует ОБА биндинга, atomic-check не поймает | **pre-existing долг** (уже сломан для nested shadow); rebind учащает; честный фикс = V2 symbol table | high (существующий долг, не блокер) |
| **D34/D184 паттерны** | `E_AMBIGUOUS_IDENT_PATTERN` parser-level; condition-биндинги = вложенный scope | ортогонально; грамматически rebind консистентен. NB: D34 отвергал `:=` за «shadowing-баги Go» — D-блоку нужно явно проговорить различие (у `:=` проблема = *случайность*, тут — явное `ro`) | low |

**Ключевая развилка реализации:** патчить каждую name-keyed подсистему — high. **Один alpha-renaming pass** после parse (`x → x__s1`, original-имя в метаданных для диагностик) — consume-checker, codegen, замыкания, verify получают уникальные имена и почти не меняются. Суммарно **medium**.

**Топ-3 риска:** (1) consume-звучность без правила R2; (2) defer-семантика (резолв на момент регистрации); (3) LSP rename (pre-existing).

---

## 4. Вердикт и правила

**Same-scope re-binding — принять к рассмотрению** (план 181), с правилами:

- **R1** — rebind только через явное `ro`/`mut`/`consume` (уже грамматика Nova; голое `=` остаётся мутацией mut-биндинга). Каждый rebind = НОВАЯ переменная; старая недоступна по имени ниже по тексту; тип может меняться.
- **R2** — **hard error** на затенение биндинга с непотреблённым consume-обязательством (`E_REBIND_LIVE_CONSUME`). Nova-эксклюзив: Rust-футган «затенённый guard» становится compile error.
- **R3** — RHS видит **старый** биндинг (нерекурсивный let; чекер уже так работает — проба 10).
- **R4** — замыкания и defer захватывают биндинг **на момент создания/регистрации** (D90 §3; env-снапшот уже даёт это для замыканий).
- **R5** — lint `W_SHADOW_UNRELATED` (warn по умолчанию): новое значение не использует старое И старое ещё живо/не потреблено. `ro x = f(x)`-pipeline — тихо.
- **R6** — параметры затенять можно (Rust-прецедент; alpha-pass делает это тривиальным).
- **R7** — cross-scope shadowing без изменений (работает сегодня); политика линта на него — вне scope.

**Даже при отказе от фичи** нужен Ф.0-минимум: явный `E_DUPLICATE_LOCAL` в чекере (вместо clang-ошибки в `.c`) + фикс false-positive D133 (проба 8b) + фикс расхождения чекер↔codegen (проба 10).

## Связь
- План: [docs/plans/181-same-scope-rebinding.md](../../plans/181-same-scope-rebinding.md) (D347)
- D-блоки: D184 (ro/mut/consume биндинги), D34 (pattern-bind; отказ от `:=`), D131/D133/D180 (consume), D22 (замыкания), D90 §3 (defer eager semantics), D24 (контракты), D217 (field cache), D297 (rename), D274 (interp unsupported)
- Родственное исследование: [2026-07-02-cross-language-syntax-gap-survey.md](2026-07-02-cross-language-syntax-gap-survey.md)
