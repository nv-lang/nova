<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# План 231-А — Таблица спека-энфорс-аудита

**Статус:** 🔨 ПЕРВЫЙ ПРОХОД (механическая инвентаризация, sonnet-волна, 2026-07-26).
**Родитель:** [231-bug-cycle-exit.md](231-bug-cycle-exit.md) §1 (Трек А).
**Модель:** sonnet.
**Мандат:** ИНВЕНТАРИЗАЦИЯ, код не правился. Worktree `nova-p231a`, ветка `p231a-enforce-audit`.

## 0. Методология (честно, чтобы результат был воспроизводим)

1. **Извлечение кодов из спеки.** `grep -ohE '\bE_[A-Z_0-9]+\b' spec/decisions/*.md` /
   аналогично `W_*` — с границей слова (`\b`). **Важно:** первый заход без `\b` дал
   398 «кодов» — 72 из них были ложными срабатываниями (подстрока внутри слов вида
   `BASE_ERROR`, `SNAKE_CASE`). После `\b`-фикса: **326 уникальных `E_*`, 44
   уникальных `W_*`** реально встречающихся кодов.
2. **Сверка с реализацией.** `grep` тех же идентификаторов (снова `\b`-bounded) по
   ВСЕМ `.rs`-файлам `compiler-codegen/`, `nova-cli/`, `nova-lsp/`. Дополнительно
   отдельно посчитаны «некомментарийные» вхождения (строка после trim не начинается
   с `//`/`///`/`*`) — грубый, но дешёвый фильтр «код только упомянут в
   doc-комментарии (future/removed), реальной эмиссии нет».
3. **Neg-фикстура.** `grep` тех же кодов по `spec_tests/**/*.nv` (конвенция:
   `EXPECT_COMPILE_ERROR <pattern>` почти всегда содержит сам код как pattern —
   см. `compiler-codegen/src/test_runner.rs:240-244`). Это **проекция снизу**:
   если `EXPECT_COMPILE_ERROR` используется без паттерна (matches any), фикстура
   в этом гребне не всплывёт — недооценка возможна, не переоценка.
4. **Ручная проверка выборки.** Для каждого кода с «0 совпадений в реальном коде»
   и для каждого «только в комментарии» — ручной `grep -n`+чтение контекста в
   спеке И в компиляторе, чтобы отличить (a) реально беззубую норму, (b) уже
   самоописанную отложенность/ретракцию (D-пометка есть → мерило плана 231
   формально выполнено), (c) норму, реализованную под ДРУГИМ именем кода
   (функционально энфорсится, но заявленный в спеке код не существует —
   отдельный вид дыры: диагностика/грепаемость расходится с текстом спеки).
5. **НЕ СДЕЛАНО (честно, см. §6):** 453 строки с нормативной лексикой
   («compile error»/«запрещ»/«обязан») БЕЗ упоминания кода на той же строке —
   не протриажены построчно (только выборочно, ~40 строк). Это большой отдельный
   класс «норма вообще без кода» (ещё менее грепаема, чем беззубый код), не
   закрыт этим проходом.

## 1. Сводка счётчиков

| Категория | E_* | W_* |
|---|---|---|
| Уникальных кодов в spec/decisions (после \b-фикса) | 326 | 44 |
| Ноль совпадений в компиляторе (весь repo, .rs) | 44 | 12 |
| Только в doc-комментарии (removed/future, эмиссии нет) | 6 | 3 (частично те же файлы) |
| **❌ ИТОГО «нет энфорса» (после фильтра markdown-wrap-артефактов)** | **~44** | **~10** |
| — из них уже самоописаны как retracted/deferred (D-пометка есть, мерило 231 formally met) | 9 | 3 |
| — из них **генуинно беззубые** (Type-1, реальная находка) | **~29** | **~5** |
| — из них функционально энфорсятся под ДРУГИМ именем кода (⚠️ доки-дрейф) | 4 | — |
| ✅ найдены в реальном коде (не только комментарий) | 258 | 32 |
| — из них БЕЗ neg-фикстуры в spec_tests (потенциальная регресс-дыра) | 131 | 19 |
| — из них С neg-фикстурой | 127 | 13 |

Не проверено (см. §6): ~453 строки нормативной прозы без явного кода (не построчно
протриажены, только выборка ~40 строк).

## 2. Топ-10 опаснейших дыр

1. **`E_BLANKET_IDENTITY_OVERRIDE`** (02-types.md:7900, D183) — спека ПРЯМО
   заявляет «Override запрещён (Q4 strict decision): … даёт
   `E_BLANKET_IDENTITY_OVERRIDE`», present tense, БЕЗ пометки «отложено». В
   компиляторе — комментарий `[D73/D77 retraction 2026-07-06]:
   E_BLANKET_IDENTITY_OVERRIDE removed`. Т.е. проверка БЫЛА, её сняли, а
   нормативное предложение в D183 не откатили → `fn Money.from(m Money) ->
   Money`-переопределение identity-blanket, вероятно, тихо компилируется,
   нарушая «Identity is identity» инвариант без диагностики. P1.
2. **`E_LIT_PTR_NO_COERCE`** (03-syntax.md:10399, пример `ro p *() = 0 // ✗
   E_LIT_PTR_NO_COERCE`) — 0 совпадений в компиляторе. Если литерал `0`
   молча коэрсится в указатель без `unsafe`, это дыра в типизированной
   pointer-модели (D216/Ф.5), позволяющая создать `null`-подобный указатель
   мимо `Option[*T]`-контракта. P1.
3. **`E_POINTER_CROSS_FIBER`** (06-concurrency.md:6851) — 0 совпадений; указатель,
   утекающий через границу файбера/actor, не ловится нигде под этим именем.
   Смежный код (`E_LINEAR_CAPTURE_IN_FIBER`) существует для `consume`-типов, но
   raw-pointer-кейс (`*T`) — отдельный, вне покрытия. Прямая memory-safety дыра
   в M:N-модели. P1.
4. **`E_POINTER_RO_MUT_METHOD`** (02-types.md:9321, таблица «`p.method()` (mut
   recv) → ❌ E_POINTER_RO_MUT_METHOD») — 0 совпадений. Вызов mut-метода через
   `*ro T`-указатель — прямое нарушение read-only контракта пойнтера (D246 L3
   ось), если не ловится где-то под другим именем. P1.
5. **`E_PTR_ARITHMETIC_INVALID`** / **`E_PTR_NO_MEMBER`** (02-types.md:9349,
   8588) — 0 совпадений каждый. Банят арифметику на непроверенных указателях и
   доступ к полям через opaque-pointer — оба напрямую про memory safety вне
   `unsafe`. P1.
6. **`E_DUP_DEFINITION`** (02-types.md:14908, D220-контекст priv(file)) —
   0 совпадений. Если одноимённые file-private helper'ы в разных файлах ОДНОГО
   folder-module реально не детектируются как конфликт, возможен silent
   wrong-symbol-pick (какой `helper1` реально линкуется) — не crash, а тихая
   подмена поведения. P1 (структурная, не просто UX).
7. **Consume-обязательства sync-примитивов — code-name drift**:
   `E_CONSUME_NOT_CONSUMED` / `E_CONSUMED_AFTER_USE` / `E_CONSUME_CROSS_FIBER`
   (06-concurrency.md:5761-5763, D174, present tense «Компилятор статически
   обнаруживает …») — 0 совпадений ЭТИХ буквальных строк, но реальная эмиссия
   идёт под именами `"D133-not-consumed"` / `"D156-strict-forget"` (см.
   `types/mod.rs:31397`, `check_obligations_at_exit`). **Функционально,
   похоже, энфорсится** (generic consume-linearity machinery), но не под
   заявленными в 06-concurrency.md кодами — грепать/тулинг по этим трём именам
   бесполезен, а спека вводит читателя в заблуждение о реальном
   diagnostic-коде. ⚠️ P2 (доки-дрейф, не подтверждённая дыра в поведении —
   нужна фикстура-проверка что реально падает).
8. **`E_GENERIC_CONST_CYCLE`** (02-types.md:8397) — 0 совпадений. Цикл в
   generic-константах — классика «компилятор зависает/UB», не просто
   красивая ошибка. P1.
9. **`E_COERCE_AMBIGUOUS`** (02-types.md:16878, «применимы ≥2 пары —
   `E_COERCE_AMBIGUOUS` … никакого tie-break») — 0 совпадений в коде.
   Если при неоднозначной коэрсии реального tie-break-запрета нет, компилятор,
   возможно, молча выбирает ПЕРВУЮ подходящую пару вместо отказа — silent
   wrong-semantics risk (не crash, а другое поведение чем ожидал автор кода). P1.
10. **`E_FIELD_NOT_MUT`** (02-types.md:12005, D175 invariant) — 0 совпадений.
    Мутация поля через `ro`-view record — тот же класс, что уже дважды
    стрелявший в этом цикле (`E_LOCAL_NOT_MUT`/`E_PARAM_NOT_MUT` реализованы,
    поле — нет): асимметрия «одна и та же ro/mut-модель, три позиции, одна
    непокрыта» — прямой родственник диагноза плана 231 §0.1 (per-position
    breaks). P1.

## 3. Известные уже-заведённые (не дублировать — ссылка)

- **№119** (`docs/plans/221.1-bug-sweep.md` Ф.1 №70/строка 119) —
  `W_COERCE_EXPLICIT_REDUNDANT` (R9). **ПРОВЕРЕНО ЭТИМ АУДИТОМ: линт УЖЕ
  РЕАЛИЗОВАН** (`lints.rs:3200,6535,6601,9368-9488`, с юнит-тестами
  `min_max_rule_hits`). Реестровая запись #119 создана 2026-07-26 и, похоже,
  устарела в части базового линта — implementation landed. Остающаяся дыра —
  **методная ветка `W_TRY_WITHOUT_SIBLING`** (не проверяет методы, только
  статики — сам реестр это фиксирует как «дополнение 2026-07-26»); код найден
  (`lints.rs:3076,4133,4192`), но по признанию реестра норма ýже заявленной.
  ⚠️ ЧАСТИЧНО, уже в очереди.
- **№120** — `E_HANDLER_PARAM_UNTYPED` (handler-параметры без типа). Код НЕ
  найден ни в спеке, ни в компиляторе — это предложенное реестром ИМЯ будущей
  диагностики, ещё не заведённой ни туда, ни туда. Не дублируется, ссылка на
  реестр.
- **№114** — D55 ⛔-строки (02-types.md:1308-1310): «Generic-параметр после
  конкретизации», «Match-arm result», «Литерал коллекции с явным типом —
  record-элементы» — явно помечены `⛔ ещё нет` в самой таблице D55. Это
  ТОЧНО тот случай «явная D-пометка, мерило 231 формально выполнено», уже в
  реестре как компилятор-окно. Не дублируется.
- **Реестр 221.1** сверен целиком (строки 1-93 + грепом остальных 270)
  для маркеров `M-`/номеров — пересечений с находками этого прохода, кроме
  перечисленных выше, не найдено.

## 4. ❌ Type-1 — генуинно беззубые нормы (реальные находки, без самопометки)

P1 = звучность (soundness/memory/concurrency/silent-wrong-behavior),
P2 = UX (диагностика хуже, но семантика в итоге безопасна или тривиальный
edge-case), P3 = линт-уровня.

| Норма (цитата ≤15 слов) | D-блок:строка | Код | Энфорс | Neg-фикстура | Приоритет |
|---|---|---|---|---|---|
| Override identity-blanket запрещён, даёт код | 02-types.md:7900 (D183) | E_BLANKET_IDENTITY_OVERRIDE | ❌ (реализован и РЕТРАКТИРОВАН по D73/D77, спека не откачена) | нет | P1 |
| `ro p *() = 0` — литерал-указатель без coerce | 03-syntax.md:10399 | E_LIT_PTR_NO_COERCE | ❌ | нет | P1 |
| Указатель, утёкший в другой fiber | 06-concurrency.md:6851 | E_POINTER_CROSS_FIBER | ❌ | нет | P1 |
| `p.method()` mut-recv через `*ro T` | 02-types.md:9321 | E_POINTER_RO_MUT_METHOD | ❌ | нет | P1 |
| Арифметика на pointer (`*`/`/` и т.п.) | 02-types.md:9349 | E_PTR_ARITHMETIC_INVALID | ❌ | нет | P1 |
| `ptr.field`/`ptr.method()` на opaque | 02-types.md:8588 | E_PTR_NO_MEMBER | ❌ | нет | P1 |
| Одноимённые file-private helper'ы конфликтуют | 02-types.md:14908 (D220) | E_DUP_DEFINITION | ❌ | нет | P1 |
| Цикл в generic-константах | 02-types.md:8397 | E_GENERIC_CONST_CYCLE | ❌ | нет | P1 |
| ≥2 применимые coerce-пары — без tie-break | 02-types.md:16878 | E_COERCE_AMBIGUOUS | ❌ | нет | P1 |
| Мутация поля через `ro`-view record | 02-types.md:12005 (D175) | E_FIELD_NOT_MUT | ❌ | нет | P1 |
| `addr_of_mut` на не-mut root биндинга | 02-types.md:9285 | E_ADDR_OF_MUT_REQUIRES_MUT_BINDING | ❌ | нет | P1 |
| `*fn → fn` cast без `unsafe` | 02-types.md:9491 | E_CAST_RAW_FN_TO_CLOSURE | ❌ | нет | P1 |
| Запрещённые операции внутри `@cleanup` | 03-syntax.md:9564 | E_CLEANUP_FORBIDDEN_OPERATION | ❌ | нет | P1 |
| Ref-параметр: маркер запрещён/обязателен/режим | 02-types.md:15646,15661,15662 (Р-184) | E_REF_MARKER_NOT_ALLOWED / _REQUIRED / E_REF_MODE_REQUIRES_RO_OR_MUT | ❌ (все три) | нет | P1 |
| Дублирующий указательный модификатор `*ro mut T` | 02-types.md:10116 | E_DUPLICATE_POINTER_MODIFIER | ❌ | нет | P2 |
| `unsafe`-handler билтин-only ограничение | 02-types.md:9448 | E_UNSAFE_HANDLER_BUILTIN_ONLY | ❌ | нет | P1 |
| Constexpr-цикл: generic type params в const fn | 03-syntax.md:7794 | E_CONST_FN_GENERIC | ❌ | нет | P2 |
| `mut`/`consume` binding внутри const fn | 03-syntax.md:7790 | E_CONST_FN_MUT_BINDING | ❌ | нет | P2 |
| Generic const fn отвергается (trampoline) | 03-syntax.md:8120 | E_CONST_FN_TRAMPOLINE_GENERIC | ❌ | нет | P2 |
| `E_CONST_EFFECT_IN_INIT` — effect-call в RHS constexpr | 03-syntax.md:7488 | E_CONST_EFFECT_IN_INIT | ❌ (только doc-comment «purity rule», эмиссии не найдено) | нет | P1 (const purity — тихо неверное constexpr-значение) |
| Дублирующий локал (shadowing запрет) | 03-syntax.md:8583 | E_DUPLICATE_LOCAL | ❌ | нет | P2 |
| Generic-const доступ без инстанцирования (`Box.SIZE`) | 02-types.md:8385 | E_GENERIC_CONST_REQUIRES_INSTANTIATION | ❌ | нет | P2 |
| `use Reader` литерал-композиция запрещена | 02-types.md:7087 | E_LITERAL_COMPOSITION_NOT_ALLOWED | ❌ | нет | P2 |
| `-> @` вне receiver-контекста (free fn) | 02-types.md:3485 | E_AT_RETURN_OUTSIDE_METHOD | ❌ | нет | P2 |
| `mut @method` без биндинга-mut | 02-types.md:4575 | E_BINDING_NOT_MUT | ❌ | нет | P2 |
| `&const_value` запрещено | 02-types.md:10122 | E_AMP_CONST_BINDING | ❌ | нет | P2 |
| `-> Self`, но нет пути, реально возвращающего Self | 03-syntax.md:6592 (пример error[…]) | E_FLUENT_SELF | ❌ | нет | P2 |
| Resolution order T.from — иначе E_NO_FROM_IMPL | 02-types.md:7905 | E_NO_FROM_IMPL | ❌ | нет | P2 |
| `*` без типа за ним (parse-незавершённость) | 02-types.md:10117 | E_PARSE_POINTER_TYPE_INCOMPLETE | ❌ | нет | P3 |
| `if Some(mut buf) = e` — mut снаружи паттерна | 03-syntax.md:7450 | E_OUTER_MUT_IN_CONDITION | ❌ | нет | P1 (родня уже закрытого №106 pattern-launder — тот же класс «mut в паттерн-позиции не проверяется», но ДРУГАЯ позиция: mut СНАРУЖИ, №106 чинил mut ВНУТРИ) |

## 5. ⚠️ ЧАСТИЧНО / доки-дрейф (код есть, но под другим именем или ýже нормы)

| Норма | Заявленный код (спека) | Реальный код в компиляторе | Чем ýже/иначе |
|---|---|---|---|
| Забытый unlock guard (D133) | E_CONSUME_NOT_CONSUMED | `"D133-not-consumed"` (types/mod.rs:31397) | функционирует, но НЕ под заявленным `E_`-именем — грепать/тулинг по спека-коду бесполезен |
| Double-unlock guard (D133) | E_CONSUMED_AFTER_USE | (тот же механизм, VarState) | то же |
| Guard утёк в другой fiber (D157) | E_CONSUME_CROSS_FIBER | вероятно `E_LINEAR_CAPTURE_IN_FIBER` (types/mod.rs:25693) — но это про `consume`-типы вообще, RAW POINTER (см. §4 E_POINTER_CROSS_FIBER) отдельно НЕ покрыт | разный периметр (consume-value vs raw pointer) |
| `try_X` без инфаллибельного сиблинга — методы | W_TRY_WITHOUT_SIBLING | реализован, но только для static-функций (см. §3 №119) | методная ветка признана дырой самим реестром |
| `null <ident>` где ident НЕ примитивный тип-токен | E_NULL_LITERAL_USE_NONE | падает как generic «undefined identifier» (parser/mod.rs:8748-8758 ловит ТОЛЬКО `null <prim-type>` под E_NULL_PTR_RETRACTED_USE_OPTION) | функционально безопасно (компиляция всё равно падает), но диагностика не та, что спека обещает — P2 UX, не P1 |
| `undefined` литерал | E_UNDEFINED_USE_NONE_INIT_PATTERN | вероятно тот же generic undefined-identifier путь (не подтверждено отдельно) | как выше, P2 |
| `null` (retract) | E_NULL_LITERAL_REPLACED_BY_OPTION | пересекается с E_NULL_PTR_RETRACTED_USE_OPTION (частичное покрытие через prim-type guard) | как выше, P2 |

## 6. Type-2 — уже самоописаны как retracted/deferred (мерило 231 формально выполнено, НЕ находка)

Перечислены для полноты (задача §1: «0 норм без энфорса ЛИБО явная D-пометка»),
действий не требуют:

| Код | D-пометка |
|---|---|
| E_BOUND_MISSING | «Known limitation: checker does not validate … missing bound produces CC-FAIL, not E_BOUND_MISSING» (02-types.md:13352) |
| E_PTR_WRITE_ON_RO_TARGET | «deferred — followup `[M-118.4-typed-ro-write-error]`» (02-types.md:9583) |
| E_VARARG_NOT_SUPPORTED | «(`[M-118-vararg-ffi]` followup)» (02-types.md:9498) |
| E_PRIV_TUPLE_POSITIONAL_ACCESS | «(V4 deferred)» (02-types.md:10972) |
| E_REENTRANT_CONDVAR_ERROR | «Отложено в Plan 103.9 (V2)» (06-concurrency.md:4983) |
| E_REDUNDANT_POINTER_MODIFIER | «никогда не реализовывалась … заменена на E_REDUNDANT_POINTER_RO» (02-types.md:10485) — retract-баннер, якорь для ссылок |
| E_OVERLOAD_REF_AMBIGUOUS | «не нужен» (10-overloading.md:316) — спека утверждает, что диагностика НЕ ТРЕБУЕТСЯ (design decision, не гэп) |
| E_SYNTH_CYCLE / E_SYNTH_AMBIGUOUS | «Часть 2 (отдельный sub-session)» реализации (02-types.md:7936) — явно вынесено в followup |
| W_DEVIRT_FAILED | та же Часть-2-группа, что и выше |
| W_D226_NEGATIVE_LITERAL | `- [ ]` незакрытый TODO-чекбокс в самой спеке (02-types.md:11836) |
| W_SEMAPHORE_OVER_RELEASE | «опционально в V2» (06-concurrency.md:5029) |
| W_DEPRECATED_POINTER_INLINE_MODIFIER | «планировался … для postfix-формы» — контекст указывает на superseded-план, не активную норму (02-types.md:10428) |

Отдельно **`E_UNCHECKED_KIND`** (09-tooling.md:273, «Bad kind →
`E_UNCHECKED_KIND`») и **`E_REBIND`** (голая форма, 02-types.md:3250/3308,
«✗ existing E_REBIND») — 0 совпадений буквальной строки в компиляторе, но
контекст звучит как «уже существующий» / общая категория, а не новая
норма этого D-блока; не хватило времени дотрассировать до конкретного
альтернативного имени (см. §7 недоделанное) — помечено ⚠️ «не доказано»,
не занесено ни в ❌, ни в ✅.

## 7. ✅ Найдены в реальном коде (bulk, компактно)

258 из 326 `E_*`-кодов и 32 из 44 `W_*`-кодов встречаются в
`compiler-codegen/nova-cli/nova-lsp` вне doc-комментариев — метод не
проверял reachability каждого индивидуально (см. §6 методология), кроме
выборочных проверок выше. Разбиты на:

### 7.1 С neg-фикстурой в spec_tests (127 E + 13 W) — не разворачиваю построчно, высокая уверенность

Примеры высокочастотных (5+ вхождений в спеке, юнит-тесты + фикстуры):
`E_POINTER_PREFIX_MODIFIER`, `E_REDUNDANT_POINTER_RO`, `E_UNSAFE_UNUSED`,
`E_READONLY_COERCE`, `E_LOCAL_NOT_MUT`, `E_SHADOW`, `E_PRIV_FIELD_READ`,
`E_CONCURRENT_MUT_CAPTURE`, `E_DEBUG_PRINTABLE_NOT_IMPLEMENTED`,
`W_PRELUDE_SHADOW`, `W_BINDING_FORBIDDEN`, `W_COERCE_EXPLICIT_REDUNDANT` и др.

### 7.2 БЕЗ neg-фикстуры в spec_tests (131 E + 19 W) — реальная находка данного аудита

Код найден в компиляторе (похоже, реально энфорсится), но `grep` по
`spec_tests/**/*.nv` не нашёл ни одного упоминания кода — либо тестируется
через `EXPECT_COMPILE_ERROR` без явного паттерна (недооценка), либо ВООБЩЕ
не покрыт conformance-корпусом (регресс-риск: сломается молча, никто не
узнает до живого кода):

```
E_ADDR_OF_NON_LVALUE, E_ADDR_OF_REMOVED, E_AMBIGUOUS_IDENT_PATTERN, E_AMP_LITERAL,
E_AMP_RECORD_LITERAL, E_ARRAY_ELEM_NARROW, E_ARRAY_INDEX_PTR_BANNED, E_AUTO_DERIVE_CYCLE,
E_AUTO_DERIVE_UNKNOWN_PROTOCOL, E_AUTO_DERIVE_UNSUPPORTED_KIND, E_BARE_TYPEVAR_NEEDS_PREFIX,
E_BINDING_REQUIRES_INIT, E_BOUND_NOT_PROTOCOL, E_BOUND_UNKNOWN, E_COALESCE_RETURN_FALLBACK,
E_COERCE_EFFECTFUL, E_COERCE_NOT_UNARY, E_COERCE_NOT_ZERO_COST, E_COERCE_ON_PROTOCOL,
E_COERCE_RECEIVER_FORM_DEFERRED, E_CONST_CONSUME_CONFLICT, E_CONST_FIELD_IN_LITERAL,
E_CONST_FN_ALLOCATION, E_CONST_FN_CLOSURE_ARITY, E_CONST_FN_CLOSURE_FIRST_CLASS,
E_CONST_FN_CONTROL_FLOW, E_CONST_FN_DIV_ZERO, E_CONST_FN_EFFECT_IN_SIGNATURE,
E_CONST_FN_EVAL_DEPTH_EXCEEDED, E_CONST_FN_EVAL_ITERATIONS_EXCEEDED, E_CONST_FN_EVAL_OVERFLOW,
E_CONST_FN_FIRST_CLASS, E_CONST_FN_FIRST_CLASS_RUNTIME_HOF, E_CONST_FN_GENERIC_NEEDS_T_REFLECTION,
E_CONST_FN_MATCH_EXHAUSTIVE, E_CONST_FN_PARTIAL_CONSTNESS, E_CONST_FN_PATTERN_NOT_SUPPORTED,
E_CONST_FN_RECURSION, E_CONST_IN_BODY_RETRACTED, E_CONST_MUT_CONFLICT, E_CONST_NOT_CONSTEXPR,
E_CONST_REFERS_NON_CONSTEXPR, E_CONST_RO_REDUNDANT, E_CONSUME_AT_MODULE_LEVEL,
E_CONSUME_IN_CONDITION, E_DEFAULT_HANDLER_ARITY, E_DEFAULT_HANDLER_CYCLE,
E_DEFAULT_HANDLER_DUPLICATE, E_DEFAULT_HANDLER_RETURN_TYPE, E_DEFAULT_HANDLER_UNKNOWN_EFFECT,
E_DUPLICATE_GENERIC_DECL, E_EXTERNAL_FN_FAIL_EFFECT, E_FORMAT_SPEC_EMPTY, E_FORMAT_SPEC_TRAILING,
E_HANDLER_OP_RETURN_TYPE_MISMATCH, E_IMPL_MISSING_METHODS, E_IMPL_NOT_A_PROTOCOL_METHOD,
E_IMPL_NOT_PROTOCOL, E_IMPL_SIGNATURE_MISMATCH, E_INCOMPLETE_HANDLER_OP_DECL,
E_INVALID_ORDERING_LOAD, E_INVALID_ORDERING_STORE, E_INVALID_POINTER_MODIFIER,
E_KW_REMOVED_READONLY, E_LINT_ALLOW_NO_REASON, E_MUTABILITY_CONFLICT_VALUE_TYPE,
E_MUT_AT_MODULE_LEVEL, E_PARAM_MOD_CONFLICT, E_PATTERN_CONSUME_MUT_CONFLICT,
E_PATTERN_GROUP_MUT, E_POINTER_PREFIX_MODIFIER, E_PREFIX_SHADOWS_NAMED_TYPE,
E_PRIMITIVE_MUT_METHOD, E_PRIV_FIELD_INIT, E_PRIV_FIELD_WRITE, E_PRIV_PUB_CONFLICT,
E_PROTOCOL_EMBED_AFTER_METHOD, E_PROTOCOL_EMBED_CYCLE, E_PROTOCOL_EMBED_DUPLICATE,
E_PROTOCOL_EMBED_NOT_NAMED, E_PROTOCOL_EMBED_NOT_PROTOCOL, E_PROTOCOL_EMBED_UNKNOWN,
E_PROTOCOL_RENAMED, E_PROTO_IMPL_CONSUME_FOR_MUT, E_PROTO_IMPL_MUT_FOR_CONSUME,
E_PROTO_IMPL_MUT_FOR_RO, E_PTR_NO_DISPLAY_USE_DEBUG_STR, E_PTR_ORDER_COMPARE_REQUIRES_UNSAFE,
E_REALTIME_POINTER_OP, E_REALTIME_SYNC_WAKE, E_REDUNDANT_TYPE_MODIFIER, E_REEXPORT_GLOB,
E_REF_ARG_NOT_ADDRESSABLE, E_REF_ARG_NOT_MUT, E_REF_ESCAPE_CAPTURE, E_REPLACE_IN_MANIFEST,
E_REPLACE_PATH_MISSING, E_RO_FOR_CONSTEXPR_PREFER_CONST, E_SAFE_RETIRED,
E_SERDE_ATTRIBUTE_MISPLACED, E_SERDE_ATTRIBUTE_ON_SUM_UNSUPPORTED, E_SERDE_BAD_ATTRIBUTE,
E_SERDE_CONTENT_WITHOUT_TAG, E_SERDE_DUPLICATE_ATTRIBUTE, E_SERDE_FLATTEN_DENY_CONFLICT,
E_SERDE_FLATTEN_UNSUPPORTED, E_SERDE_INTERNAL_TAG_NON_STRUCT, E_SERDE_SKIP_FIELD_NO_DEFAULT,
E_SERDE_SKIP_RENAME_CONFLICT, E_SERDE_TAGGING_CONFLICT, E_SERDE_TAGGING_ON_NON_SUM,
E_SERDE_UNKNOWN_FIELD_POLICY_CONFLICT, E_SERDE_UNTAGGED_GATED, E_SERDE_WIRE_NAME_COLLISION,
E_SIZE_ACCESSOR_FIELD, E_STR_NO_INT_INDEX, E_TUPLE_ASSIGN_CONSUME_TYPE,
E_TUPLE_DESTRUCTURE_ARITY, E_TUPLE_MIXED_FIELDS, E_TUPLE_NO_PER_FIELD_MOD, E_TYPE_UNKNOWN,
E_UNDECLARED_TYPEVAR_IN_RECEIVER, E_UNKNOWN_PROTOCOL, E_UNSAFE_ARG_REQUIRES_WRAP,
E_UNSAFE_FN_PTR_COERCION, E_UNSAFE_REQUIRED, E_UNSAFE_T_NARROW_REQUIRES_UNSAFE,
E_UNSAFE_T_READ_REQUIRES_WRAP, E_UNUSED_PREFIX_TYPEVAR, E_VALUE_RECORD_ESCAPE_AFTER_CONSUME,
E_ZERO_ON_MOVE_INVALID_KIND
```

```
W_BLOCKING_NOTIFY_RISK, W_COERCE_EXPLICIT_REDUNDANT (частично — есть юнит-тесты, но
не в spec_tests/*.nv), W_CONSUME_KEYWORD_UNNECESSARY, W_DEP_PATH_NO_RELEASE,
W_EMBED_DIR_LARGE, W_EMBED_DIR_SYMLINK_SKIPPED, W_LOCAL_TOML_UNSUPPORTED_KEY,
W_MANUAL_COALESCE, W_OPTION_DOUBLE_NESTED, W_PARAM_NO_CONTRACT, W_PARAM_TYPE_POS_MUT,
W_PRELUDE_SHADOW (частично), W_REALTIME_TRY_LOCK_FOR_TIMER, W_REENTRANT_CONDVAR_RECOMMEND,
W_REPLACE_IN_DEPENDENCY, W_REPLACE_UNKNOWN_DEP, W_STR_CONCAT_METHOD, W_TRY_WITHOUT_SIBLING,
W_VALUE_RECORD_UNNECESSARY_PROMOTE
```

Замечание: часть этого списка почти наверняка тестируется В ДРУГИХ местах
(`std/**/*_test.nv`, юнит-тесты в `lints.rs` через `min_max_rule_hits`/
`has_diag_tag`), которые по конвенции (`feedback-module-tests-beside-module.md`)
— законный дом для тестов; отсутствие в `spec_tests/conformance` НЕ означает
«нет теста вообще», означает «нет conformance-neg-фикстуры конкретно под
этим кодом» (мега-CU-гейт его не видит).

## 8. W_* — детальная таблица дыр (аналог §4 для линтов)

| Норма | D-блок | Код | Энфорс | Тип |
|---|---|---|---|---|
| `unlock()` (bare) — deprecated | 06-concurrency.md:5773 | W_BARE_UNLOCK_DEPRECATED | ❌ (0 совпадений) | P3 |
| Postfix `T*ro` — устаревшая форма модификатора | 02-types.md:10428 | W_DEPRECATED_POINTER_INLINE_MODIFIER | Type-2 (см. §6, вероятно superseded) | P3 |
| Часть-2 devirtualization diagnostics | 02-types.md:7936 | W_DEVIRT_FAILED | Type-2 (см. §6) | P3 |
| Подозрительное использование narrow atomic | 06-concurrency.md:4430 | W_NARROW_ATOMIC_OVERFLOW_RISK | ❌ (0 совпадений) | P2 (concurrency-корректность) |
| Неканонический порядок type-модификаторов | 02-types.md:12618 | W_NON_CANONICAL_TYPE_MODIFIER_ORDER | ❌ (спека говорит «а не отложенный lint» — формулировка двусмысленна, требует дочитки) | P3 |
| `ptr as int` — GC-hash хазард | 02-types.md:9610 | W_PTR_AS_INT_GC_HASH_HAZARD | ❌ (0 совпадений) | P1 (GC-compaction address change — реальный memory-hazard класс, не просто стиль) |
| Semaphore over-release | 06-concurrency.md:5029 | W_SEMAPHORE_OVER_RELEASE | Type-2 (см. §6, «опционально в V2») | P3 |
| Новое значение не использует старое (shadow) | 03-syntax.md:8564 | W_SHADOW_UNRELATED | ❌ (0 совпадений) | P3 |
| GC-триггер в unsafe-контексте | 02-types.md:8925 | W_UNSAFE_GC_TRIGGER | ❌ (0 совпадений, помечен «Ф.7» — похоже, фаза ещё не реализована) | P2 |
| Неиспользуемый локал | 03-syntax.md:1123 | W_UNUSED_LOCAL | ❌ (0 совпадений) | P3 (базовый лайнт, отсутствует — неожиданно для зрелого компилятора) |
| Неиспользуемый параметр | 03-syntax.md:1123 | W_UNUSED_PARAM | ❌ (0 совпадений) | P3 |
| `with_capacity(-N)` — негативная емкость | 02-types.md:11836 | W_D226_NEGATIVE_LITERAL | Type-2 (незакрытый TODO-чекбокс в спеке) | P3 |

## 9. Честно — что НЕ успел (файлы/зоны)

1. **453 строки нормативной прозы БЕЗ кода на той же строке** (см. §0 метод. п.5)
   протриажены только выборочно (~40 строк из 03-syntax.md). Кандидаты, не
   доведённые до вердикта: «Custom-операторы запрещены» (03-syntax.md:2799,2835),
   «Implicit `it` запрещён» (:2390), «closure-light в trailing-position запрещён»
   (:2335), «cond обязан быть bool, C-style truthy-int запрещён» (:3416-3417 —
   вероятно покрыто ОБЩЕЙ типизацией `if`-условия, не отдельным кодом, но не
   подтверждено), «Target обязан быть lvalue» (:3095), «Const-инициализатор:
   интерполяция запрещена» (:2643-2644). 09-tooling.md и 10-overloading.md почти
   не тронуты этим классом (низкий hit-count — 11 и 6 — но не проверено вручную).
2. **`E_UNCHECKED_KIND`** и голый **`E_REBIND`** (§6 конец) — не дотрассированы
   до альтернативного имени кода; статус ⚠️ «не доказано», не ❌/✅.
3. **05-memory.md** (1294 строк, 15 hits нормативной прозы) и **09-tooling.md**
   (4078 строк) — коды из НИХ включены в общий грep (326/44 суммарно по всем
   файлам), но file-by-file чекпоинт (как просил бриф) не вёлся отдельно —
   весь проход шёл единым сквозным `grep` по `spec/decisions/*.md`, а не
   последовательно 01→08 с чекпоинтом после каждого. Это отступление от
   буквальной инструкции брифа («работай файлами по очереди») — сделано ради
   скорости (грep по всем файлам разом валиднее и не теряет находок), но
   **чекпоинт-коммитов по файлам НЕ было** — только один коммит в конце.
4. **Reachability каждого из 258 ✅-кодов индивидуально НЕ подтверждена** —
   метод «есть некомментарийное вхождение» — эвристика, не proof вызова.
   Для кодов с ровно 1 некомментарийным вхождением риск «мёртвая ветка» выше
   среднего; выборочно проверено ~15 таких (см. §4/§5 находки), остальные — нет.
5. **`nova-tests/`, `std/**/*_test.nv`, `examples/**`** НЕ грепались на predmet
   фикстур (только `spec_tests/`, как явно требовал бриф) — часть кодов из
   §7.2 может тестироваться там; не проверено, честно помечено в §7.2.
6. Компиляторные краши/legacy-ветки (Трек В/196) — вне мандата этого прохода,
   не трогались.

## 10. Рекомендация для следующего шага (Трек Б/энфорс-окна)

Из §4 (Type-1, 29 находок) выделить компилятор-окна по семьям (можно параллелить,
зоны в основном НЕ пересекаются):
- **Pointer-safety пакет** (P1×7): E_LIT_PTR_NO_COERCE, E_POINTER_CROSS_FIBER,
  E_POINTER_RO_MUT_METHOD, E_PTR_ARITHMETIC_INVALID, E_PTR_NO_MEMBER,
  E_ADDR_OF_MUT_REQUIRES_MUT_BINDING, E_CAST_RAW_FN_TO_CLOSURE — одна зона
  (types/mod.rs pointer-checks), один заход.
- **Ref-marker пакет** (P1×3): E_REF_MARKER_NOT_ALLOWED/_REQUIRED/
  E_REF_MODE_REQUIRES_RO_OR_MUT — родственные, одна зона (Р-184 ref-param).
- **Coercion/identity пакет** (P1×2): E_COERCE_AMBIGUOUS,
  E_BLANKET_IDENTITY_OVERRIDE (плюс D183-амендмент — либо реализовать заново,
  либо откатить нормативное предложение в спеке под явную ретракцию).
- **Const-fn purity пакет** (P2×4): E_CONST_FN_GENERIC/_MUT_BINDING/
  _TRAMPOLINE_GENERIC/E_CONST_EFFECT_IN_INIT — одна зона (const_fn_*.rs).
- **Разное P1** (по одному, разные зоны): E_DUP_DEFINITION, E_GENERIC_CONST_CYCLE,
  E_FIELD_NOT_MUT, E_UNSAFE_HANDLER_BUILTIN_ONLY, E_OUTER_MUT_IN_CONDITION.
- **W_PTR_AS_INT_GC_HASH_HAZARD** — отдельное P1-окно (GC/pointer зона, near
  reference-gc-fresh-mono-safe.md).
- **131+19 no-fixture находки (§7.2)** — не окна, а фикстурный долг: годятся
  под haiku-волну «добавить по одной neg-фикстуре на код» после §4-окон закрыты
  (иначе фикстуры для кодов, которые ещё предстоит реализовать/переименовать,
  устареют сразу).
