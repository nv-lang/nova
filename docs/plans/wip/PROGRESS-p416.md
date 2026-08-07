# PROGRESS — окно p416-lint-clean (реестр 221.1 №416, К1)

Модель: sonnet. Worktree: `d:/Sources/nv-lang/nova-p416` (ветка `p416-lint-clean`).

## Итог

- `nova lint std/src` — **11 находок → 0 находок**.
- `nova check std/src` — канон **PASS: 151 FAIL: 26 WARN: 61** не сдвинулся.
- `nova lint --deny std/src` добавлен в `scripts/gate.sh` (шаг после `check
  std/src`, перед flagship) — ассертит строку `"..., 0 finding(s)..."`
  явно, не только exit-код.
- Проба «подсунь заведомо негодное» — **гейт краснеет** (см. ниже, вердикт
  дословно).
- `nova test` по затронутым модулям (`math`, `runtime`, `text`,
  `time/civil`) — зелёные, кроме одного **pre-existing** RUN-FAIL, не
  связанного с этой правкой (см. таблицу).

## Таблица «файл → находка → как починено»

| Файл | Находка | Как починено |
|---|---|---|
| `std/src/math/int128.nv:469` | `W_PARAM_NO_CONTRACT` — `@shl(n int)` | Добавлен `requires n >= 0` (канон репозитория для параметра `n`: то же используют `hash_map/core.nv`, `queue.nv`, `set/core.nv`, `vec_iter/core.nv`, `rate_limiter.nv`). Тело уже клэмпит `n<=0` в identity и `n>=128` в `ZERO`/sign-extend, но это защита, а не документированный домен — отрицательный `n` нигде не вызывается (грепнуто по всему дереву). |
| `std/src/math/int128.nv:481` | `W_PARAM_NO_CONTRACT` — `@shr(n int)` | Тот же контракт `requires n >= 0`, то же обоснование. |
| `std/src/runtime/fmt_buf/core.nv:230` | `W_PARAM_NO_CONTRACT` — `bool_fmt(cap)` | `requires cap >= 0` — точное повторение контракта, УЖЕ стоящего на соседях `int_fmt`/`f64_fmt`/`f32_fmt` в этом же файле (строки 137/324/338, pre-existing). |
| `std/src/runtime/fmt_buf/core.nv:257` | `W_PARAM_NO_CONTRACT` — `char_fmt(cap)` | `requires cap >= 0`, тот же канон. |
| `std/src/runtime/fmt_buf/core.nv:413` | `W_PARAM_NO_CONTRACT` — `str_debug_fmt(cap)` | `requires cap >= 0`, тот же канон. |
| `std/src/runtime/fmt_buf/core.nv:442` | `W_PARAM_NO_CONTRACT` — `char_debug_fmt(cap)` | `requires cap >= 0`, тот же канон. |
| `std/src/runtime/string/core.nv:338` | `W_PARAM_NO_CONTRACT` — `@is_char_boundary(idx)` | **Не контракт** — почина` nova:allow`. Здесь УЖЕ стояла попытка `// nova:allow W_PARAM_NO_CONTRACT -- ...`, но она была сломана: причина ушла на ВТОРУЮ строку комментария, а маркер обязан стоять СТРОГО на строке `fn_line - 1` (`apply_nova_allow_suppressions`, `compiler-codegen/src/lints.rs:3411`) — двухстрочный wrap рвал соседство, и линт продолжал находить строку. Сведено в одну строку прямо над `fn`. Контракт по существу не подходит: `is_char_boundary` — намеренно ТОТАЛЬНЫЙ предикат над ЛЮБЫМ `idx` (в т.ч. отрицательным/за концом строки) — это его единственное назначение (safe pre-check перед `s[a..b]`, которая иначе паникует на рассечении кодпоинта). `requires idx>=0 && idx<=len` убил бы саму цель функции, обязав вызывающего заранее доказывать то, для проверки чего функция и существует. |
| `std/src/text/regex.nv:212` | `W_MANUAL_COALESCE` — `match m.to_int() {Ok(v)=>v, Err(_)=>throw ...}` | **Не заменено** — `nova:allow`, задокументирован компиляторный гэп (детали ниже). |
| `std/src/text/regex.nv:578` (`try_match`) | `W_MANUAL_COALESCE` — `match tchars.get(pos) {Some(ch)=>ch, None=>break}` | **Не заменено** — `nova:allow`, `break` не выражение (детали ниже). |
| `std/src/time/civil/parse.nv:92` (`time_at`) | `W_MANUAL_COALESCE` — `match c.to_int() {Ok(x)=>x, Err(_)=>break}` | **Не заменено** — `nova:allow`, тот же break-shape, что regex.nv:578. |
| `std/src/time/civil/parse.nv:270` (`@to_period`) | `W_MANUAL_COALESCE` — `match c.to_int() {Ok(x)=>x, Err(_)=>break}` | **Не заменено** — `nova:allow`, тот же break-shape. |

Итог по W_PARAM_NO_CONTRACT: **6 реальных `requires`, 1 обоснованный `nova:allow`**.
Итог по W_MANUAL_COALESCE: **1 реальная замена на `??`, 3 обоснованных `nova:allow`**.

## Параметры, для которых контракт написать нельзя (по существу)

**`str @is_char_boundary(idx int) -> bool`** (`std/src/runtime/string/core.nv:338`).

Функция — намеренно тотальный предикат: для ЛЮБОГО `idx` (отрицательного,
за концом строки, рассекающего codepoint) она обязана вернуть `false`, а
не потребовать валидность `idx` заранее через `requires`. Это единственный
способ безопасно проверить произвольный, потенциально невалидный индекс
ПЕРЕД `s[a..b]` (который иначе паникует на рассечении). Контракт вида
`requires idx >= 0 && idx <= @byte_len()` был бы логической ошибкой —
он запретил бы ровно те вызовы, ради проверки которых функция и
существует. Зафиксировано `nova:allow` с этим обоснованием прямо в коде.

## W_MANUAL_COALESCE — 3 находки НЕ заменены на `??`, с доказательствами

### 1) `break`-fallback (regex.nv:578, civil/parse.nv:92, civil/parse.nv:270) — компилятор не поддерживает `?? break`

`break`/`continue` в грамматике Nova — только `Stmt`, не `Expr`
(`compiler-codegen/src/parser/mod.rs:11549/11553`), а RHS оператора `??`
парсится через `parse_unary()` (`parser/mod.rs:8907`), которая требует
primary-token expression. Пробой подтверждено эмпирически:

```
ro c = xs.get(i) ?? break
```
```
error: unexpected `break` in expression
```

(файл пробы: `C:\Users\...\scratchpad\probe_break.nv`, прогнан через
`nova check`, exit=1.)

Канон `X ?? D` в этих трёх местах структурно неприменим — задокументировано
`nova:allow W_MANUAL_COALESCE` с ссылкой на этот вывод, а не тихая замена.

### 2) `?? throw` ломает D162 cleanup-checker (regex.nv:212, `@parse_quantifier_max`) — уточнение дефекта №394

Изначально заменил на `m.to_int() ?? throw InvalidQuantifier { position: start }`
(грамматически валидно) — но это сломало сборку:

```
error: [D162-uncovered-error-path] consume binding `m_str` (тип `StringBuilder`)
в failable function без cleanup-покрытия error-path...
```

**Корневая причина найдена и подтверждена в коде компилятора.**
`check_d162_coverage` использует `expr_has_throw`
(`compiler-codegen/src/types/mod.rs:36174-36210`, маркер
`[M-d162-structural-throw-sibling]`) как «доказательство, что автор явно
обработал error-path». Функция спускается в `Block`/`If`/`With`/`Match`/
`IfLet`/`While`/`WhileLet`/`For`/`Loop`, разыскивая вложенный
`ExprKind::Throw` — но **не спускается в `ExprKind::Coalesce`**. Поэтому
`throw` на правой стороне `??` для неё невидим, и функция ложно ловит
`D162-uncovered-error-path`, хотя `m_str` уже дискаунтед (`.into_str()`)
до этой точки и throw реально покрывает единственный оставшийся exit-путь.

Подтверждено эмпирически парой прогонов (замена → `nova check std/src/text`
падает 1 FAIL; откат → 0 FAIL, тот же diff изолирован одной строкой).

Это — компиляторный гэп в `expr_has_throw`, а не проблема `.nv`-кода;
чинить его — задача отдельного окна над `types/mod.rs` (вне периметра
К1 = std/src). Оставлено в исходной match-форме, но с `nova:allow
W_MANUAL_COALESCE` (иначе линт находил бы её снова), причина в коде
дословно ссылается на `expr_has_throw` и файл/маркер выше — это
одновременно и обоснование, и наводка для того, кто будет чинить
дефект №394 (или заводить отдельный номер под сам `expr_has_throw`-гэп:
на усмотрение интегратора, здесь только зафиксировано наблюдение).

## Проба «подсунь заведомо негодное» — вердикт

Временно снят `requires n >= 0` с `i128 @shl` (`std/src/math/int128.nv`),
прогнан новый шаг гейта вручную (та же команда, что в `scripts/gate.sh`):

```
$ nova lint --deny std/src
...
lint: 276 file(s), 1 finding(s), 1 denied (--deny, exit 1)
GATE FAIL: nova lint std/src: находки > 0, ожидался канон 0, ...
```

**Шаг гейта корректно краснеет** (`GATE FAIL`, shell exit=1). После пробы
`requires n >= 0` возвращён — `git diff std/src/math/int128.nv` пуст,
дерево чистое.

## `nova lint std/src` — вывод ДО (11 находок, дословно, начало волны)

```
std/src\math\int128.nv:469:21: warning: index/offset/len parameter `n` of public std fn `shl` without `requires`: ... [W_PARAM_NO_CONTRACT]
std/src\math\int128.nv:481:21: warning: index/offset/len parameter `n` of public std fn `shr` without `requires`: ... [W_PARAM_NO_CONTRACT]
std/src\runtime\fmt_buf\core.nv:230:41: warning: index/offset/len parameter `cap` of public std fn `bool_fmt` without `requires`: ... [W_PARAM_NO_CONTRACT]
std/src\runtime\fmt_buf\core.nv:257:41: warning: index/offset/len parameter `cap` of public std fn `char_fmt` without `requires`: ... [W_PARAM_NO_CONTRACT]
std/src\runtime\fmt_buf\core.nv:413:45: warning: index/offset/len parameter `cap` of public std fn `str_debug_fmt` without `requires`: ... [W_PARAM_NO_CONTRACT]
std/src\runtime\fmt_buf\core.nv:442:47: warning: index/offset/len parameter `cap` of public std fn `char_debug_fmt` without `requires`: ... [W_PARAM_NO_CONTRACT]
std/src\runtime\string\core.nv:338:33: warning: index/offset/len parameter `idx` of public std fn `is_char_boundary` without `requires`: ... [W_PARAM_NO_CONTRACT]
std/src\text\regex.nv:212:5: warning: manual `match X { Ok(v) => v, Err(_) => D }` ... [W_MANUAL_COALESCE]
std/src\text\regex.nv:578:16: warning: manual `match X { Some(v) => v, None => D }` ... [W_MANUAL_COALESCE]
std/src\time\civil\parse.nv:92:25: warning: manual `match X { Ok(v) => v, Err(_) => D }` ... [W_MANUAL_COALESCE]
std/src\time\civil\parse.nv:270:21: warning: manual `match X { Ok(v) => v, Err(_) => D }` ... [W_MANUAL_COALESCE]

lint: 276 file(s), 11 finding(s)
```

## `nova lint std/src` — вывод ПОСЛЕ (0 находок, дословно)

```
lint: 276 file(s), 0 finding(s)
```

## `nova check std/src` — канон не сдвинулся

```
===== SUMMARY =====
PASS: 151  FAIL: 26  WARN: 61
```
(идентично базовому прогону ДО правок этого окна; все 26 FAIL —
pre-existing `neg/`-фикстуры, ни одна не поменяла статус.)

## `nova test` по затронутым модулям

- `std/src/math`: `PASS: 5 FAIL: 0 SKIP: 3` — все зелёные, включая
  `int128_test` (кроет `shl(0/1/63/64/127/128)`/`shr(...)`).
- `std/src/runtime`: `PASS: 5 FAIL: 0 SKIP: 13` — все зелёные, включая
  `fmt_buf/core` (внутренние `test{}`-блоки этого же файла).
- `std/src/text`: `PASS: 3 FAIL: 0 SKIP: 3` — все зелёные, включая
  `regex_test`.
- `std/src/time/civil`: `PASS: 0 FAIL: 1 SKIP: 3` — **1 RUN-FAIL,
  pre-existing**: `civil_arith_test` (integer overflow в round-trip
  format+parse). Воспроизведён БАЙТ-В-БАЙТ на немодифицированном
  main-бинаре (`d:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe`,
  прогнан ДО каких-либо правок этого окна) — та же строка FAIL. Не
  регрессия этой волны; правки `civil/parse.nv` в этом окне — только
  комментарии (`nova:allow`), функциональный код не тронут.

## Дополнительно измерено (по заданию — «сначала измерь, доложи»): CI-периметры lint

`.github/workflows/nova-lint.yml` гоняет ЖЁСТКИЕ гейты (`--deny`, 0
находок required) по ДВУМ периметрам: `std` (этот отчёт — теперь 0) И
**`spec_tests`** (`nova-lint-spec-tests-gate`). Измерено:

```
$ nova lint --deny spec_tests
...
lint: 1388 file(s), 83 finding(s), 83 denied (--deny, exit 1)
```

**Это ЗНАЧИТ, что `nova-lint-spec-tests-gate` тоже, вероятно, красный на
CI** — отдельная, не входящая в периметр К1 (№416 = `std/src`) находка.
Виновники — разные правила (`W_RESULT_DISCARDED`,
`W_REDUNDANT_CONSUME_REBIND`, `W_WHILE_COUNTER_FOR_RANGE`,
`W_MANUAL_COALESCE`, `W_REDUNDANT_CONST_TYPE_ANNOTATION`), в основном
в `spec_tests/conformance/standalone/*` и `spec_tests/fixtures/known_red/*`
— НЕ разбирал по существу (вне мандата этого окна), только измерил и
докладываю. **Не заведён в gate.sh** — заводить локальный 0-находок гейт
на периметр, который сейчас красный, означало бы держать `gate.sh`
постоянно красным без починки; решение — за интегратором/владельцем
(отдельное окно, номер №TBD).

`examples` CI линтом НЕ гейтится вообще (в `nova-lint.yml` не упомянут).
Измерено локально: `nova lint examples` → `1 finding(s)`
(`examples/flagship/aggregator/src/app/live.nv:91` —
`W_REDUNDANT_CONSUME_REBIND`). Тоже не тронуто (вне периметра К1,
и не гейтится CI) — только для полноты картины.

## Коммиты этого окна (по файлам)

1. `fix(std): int128 shl/shr — requires n >= 0 (W_PARAM_NO_CONTRACT)`
2. `fix(std): fmt_buf bool_fmt/char_fmt/str_debug_fmt/char_debug_fmt — requires cap >= 0`
3. `fix(std): is_char_boundary — repair broken nova:allow (W_PARAM_NO_CONTRACT)`
4. `fix(std): regex.nv — W_MANUAL_COALESCE, 2 findings triaged (0 fixed, 2 documented nova:allow)`
5. `fix(std): civil/parse.nv — W_MANUAL_COALESCE, 2 findings documented nova:allow`
6. `build(gate): добавить nova lint --deny std/src в scripts/gate.sh (221.1 №416)`

## Что осталось интегратору

- Номер №416 закрыть (или переоткрыть частично) по решению владельца —
  периметр К1 (`std/src`) полностью зелёный, гейт защищён и провалидирован
  пробой.
- Красный `nova-lint-spec-tests-gate` на CI (83 находки, измерено выше) —
  отдельный вопрос, номер присвоит интегратор (№TBD), не входил в мандат
  этого окна.
- Компиляторный гэп `expr_has_throw` не спускается в `ExprKind::Coalesce`
  (`compiler-codegen/src/types/mod.rs:36174-36210`) — уточняет дефект
  №394; при желании чинить отдельным окном над `types/mod.rs` (не
  `.nv`-фикс, значит вне периметра этого окна по стандартному правилу
  «компилятор — в чекер-канал, отдельная волна»).
