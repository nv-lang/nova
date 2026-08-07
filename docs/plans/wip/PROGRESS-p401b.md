<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# PROGRESS — окно p401b-p67-class

Ветка `p401b-p67-class`, worktree `d:/Sources/nv-lang/nova-p401b`. Модель: sonnet.

Задача: реестр 221.1 №401, ПЕРЕОТКРЫТ 2026-08-07 — закрыть весь КЛАСС
`[P67-LEGACY]` internal-error (компилятор падает вместо диагностики), не
очередной носитель.

## Измеренное число мест

Брифовая цифра «38» — **строчных упоминаний** подстроки `P67-LEGACY` в
`emit_c.rs` (`grep -c "P67-LEGACY" compiler-codegen/src/codegen/emit_c.rs` →
`38`, проверено дословно), из них комментариев/трассировки — большинство.
**Реальных мест паники** (`panic!("[P67-LEGACY] ...")`) — измерено отдельно:

```
grep -rn 'panic!("\[P67-LEGACY\]' compiler-codegen/src/
```

→ **11** сайтов, все в `compiler-codegen/src/codegen/emit_c.rs`, все внутри
двух больших легаси-диспетчеров (`infer_call_ret_c`,
`infer_expr_c_type_legacy`/`infer_expr_c_type`) из класса «checker должен
был проаннотировать тип выражения (compiler-conventions.md §0), но канал
(`resolved_types`/`resolved_callees`) пуст, и ни одна из легаси-эвристик
дальше по цепочке не сработала». Соседняя, старшая метка `[P67]` (без
`-LEGACY`, 13 сайтов, «nova_int collapse»/IntLit-без-контекста) — **другой,
отдельный класс**, вне мандата этого окна (не трогался).

## Группы (по причине, не по симптому)

| # | Группа | Сайты (emit_c.rs, до правки) | Достигается формой | Вердикт |
|---|---|---|---|---|
| 1 | Path-call return type unknown (`Type.method(...)`, 2/1/0-частный путь) | 59712, 59715, 59719 | `Io.write_out(...)` без `import std.io` — стартовый носитель брифа попадает СЮДА (59712, `parts.len()==2`, `method_name="write_out"`) | конвертировано в честную диагностику |
| 2 | Member-call return type unknown (`obj.method(...)`) | 59569 | зеркало группы 1 для member-формы (не подтверждён живым носителем, но структурно тот же терминал того же диспетчера) | конвертировано |
| 3 | Result T/E unknown | 59181 | `resolve_result_te` не смог вывести оба типа `Result[T,E]` для метода-цепочки | конвертировано |
| 4 | Index element type unknown | 60489 | `arr[i]` на ресивере, чей elem-тип не выводится ни одной из легаси-эвристик | конвертировано |
| 5 | Ident not in var_types (2 идентичных близнеца) | 60732, 61839 | голый идентификатор в value-позиции без записи в `var_types`/`resolved_types` | конвертировано |
| 6 | Try/Bang on Result: Ok type unknown | 62317 | `expr!!`/`expr?` на `Result`, где Ok-тип не выводится | конвертировано |
| 7 | Turbofish type_arg lowering failed | 57368 | `type_ref_to_c` вернул `Err` для типового аргумента turbofish (napr. ретрактированный `usize`/`ptr`) | конвертировано |
| 8 | `infer_expr_c_type_legacy` финальный wildcard | 62443 | ЛЮБОЙ необработанный `ExprKind` | **НЕ трогался** — уже `#[cfg(debug_assertions)]` **и** под `NOVA_STRICT_LEGACY` env; в release-сборке физически не компилируется, в debug молчит по умолчанию. Не часть класса «компилятор падает у пользователя» |

10 из 11 живых сайтов переведены на единый механизм; 11-й (группа 8) уже
безопасен по конструкции — оставлен как есть с пояснением в отчёте (браться
за него — расширять периметр задачи без пользы: он и так не падает).

## Чем подход отличается от провалившейся попытки у №81

`types/mod.rs` (~14755, комментарий `[M-p81-unknown-static-receiver-silent-p67]`)
документирует: общий checker-диагноз «Type not declared/imported» для
нерезолвленного `Type.method(...)` Path-вызова **был опробован и откачен** —
регресс `nova check std/src` на **42 файла** ложных позитивов (cross-module
top-level `const`-ресиверы вроде `I64_MIN.to_nanos()`; чисто-рантайм
intrinsic-неймспейсы без `.nv`-декларации типа вообще —
`ChanReader`/`Channel`/`CancelToken`/`StringBuilder`/`WriteBuffer`/
`ReadBuffer` — резолвятся ТОЛЬКО через хардкод-диспатч `emit_c.rs`, чекеру
не видны).

Стартовый носитель этого окна (`Io.write_out("")` без `import std.io`)
структурно — **тот же самый класс** нерезолвленного Path-вызова. Я
подтвердил это пробой: `nova check` даёт `PASS` без импорта (чекер молчит —
`method_overloads("Io","write_out")` возвращает `None`, ветка `match
overloads { None => return, ... }` в `types/mod.rs` тихо выходит без
диагностики и без записи в `resolved_types`/`resolved_callees` — ЭТО и есть
задокументированный, намеренно не тронутый гэп №81); `nova build`/`nova
test` тем временем падали `[P67-LEGACY]`, потому что `effect_schemas["Io"]`
в `emit_c.rs` строится ТОЛЬКО из деклараций, реально присутствующих в
текущем compile-unit (тот же импорт-гейт), — а с `import std.io` панике
взяться неоткуда: полный прогон до `.exe` и его вывод `hi` подтверждают
(см. «Пробы» ниже).

**Отличие: я НЕ трогал чекер вообще.** Вместо повторения общего
checker-диагноза (тот же риск того же 42-файлового регресса — форма
идентична) фикс целиком на КОДОГЕН-стороне: терминальная паника
(`panic!("[P67-LEGACY]...")`) заменена на `panic!` с честным, кодированным
сообщением (`[E_CODEGEN_TYPE_UNKNOWN]` + `file:line:col` + подсказка «обычно
не хватает `import`»), пойманным новой инфраструктурой `catch_unwind`
(см. ниже) — а НЕ новой checker-диагностикой. `nova check std/src`
до/после — **151/26/61 без изменений** (измерено, не предположено) — прямое
доказательство, что чекер не тронут ни байтом.

Это ровно то, что бриф явно санкционировал как приемлемый исход для
«остатка» (Шаг 4): «замена паники на честную диагностику с кодом», а не
попытка дорезолвить сам гэп чекера.

## Вторая находка (не в брифе, но той же «цены» ради): обрыв прогона

Реестр №401 явно называет «цену» бага: падение обрывает ВЕСЬ прогон,
пряча всё, что идёт следом. Первая версия фикса (честное сообщение через
`std::process::exit(1)` напрямую) **не решала эту часть** — проверено
пробой: 4-файловый батч с одним плохим носителем печатал диагностику и
**останавливался без сводки**, три хороших файла после него ни разу не
запускались (`process::exit` не перехватывается `catch_unwind` в принципе).

Добавлена per-unit защита от паники:
- `compiler-codegen/src/test_runner.rs`: `catch_unit_panic` (pub) —
  оборачивает ОДИН compile-unit'а кодоген-вызов в `catch_unwind`,
  превращая пойманную панику в ТОТ ЖЕ `Err(String)`, что `codegen_to_c`
  уже возвращает для обычной ошибки компиляции (`E_FFI_C_NAME_OVERLOAD_
  CONFLICT` и подобные) — весь нижестоящий код (отчёт FAIL, запись
  results-файла, продолжение батча) не изменился ни строкой: он уже
  корректно обрабатывает `Err` (проверено контрольной пробой — обычная
  синтаксическая ошибка в одном файле директории НЕ останавливает батч;
  теперь внутренние паники ведут себя так же).
- `nova-cli/src/main.rs`: `process::exit(101)` перенесён ИЗ хука паники в
  новый внешний `run_catching` (оборачивает `run` в свой `catch_unwind`) —
  хук не может быть перехвачен, если сам вызывает `process::exit`
  безусловно, поэтому перенос был обязателен. Хук получил ранний выход по
  `catching_panic_active()` (per-thread флаг — тихо, когда где-то выше по
  ЭТОМУ ЖЕ потоку уже стоит `catch_unit_panic`). `cmd_build` (одиночный
  `nova build`) тоже обёрнут той же `catch_unit_panic` (сделана `pub`,
  переиспользована, не продублирована) — вместо баннера «report a bug»
  теперь `error: codegen error: [E_CODEGEN_TYPE_UNKNOWN] ...`, exit=1.

Проверено пробой (временный env-гейтнутый паник-зонд ВНЕ обёрнутой
области, добавлен/удалён только для этой проверки): паника, ускользнувшая
из ЛЮБОГО `catch_unit_panic`, по-прежнему печатает старый баннер и
`exit=101` — байт-в-байт как до окна.

## Приёмка

### 1. Таблица

См. «Группы» выше — 10/11 закрыто честной диагностикой, 1/11 уже безопасен
(debug+opt-in-only, не требует правки).

### 2. Проба «подсунь заведомо негодное» — дословные вердикты

**Проба А (откат ОДНОГО сайта на сырой текст паники, инфраструктура
перехвата не тронута):** негативная фикстура
`spec_tests/conformance/neg/m221_401_path_call_unimported_effect_neg.nv`
перевернулась на
```
NEG-WRONG-MSG  ...  # expected pattern 'E_CODEGEN_TYPE_UNKNOWN' not found in: [INTERNAL-PANIC] [P67-LEGACY] Path call return type unknown for method=write_out — checker must annotate (compiler-conve
```
Восстановлено, `diff` против бэкапа — идентично, повторный прогон —
`PASS: 1 FAIL: 0`.

**Проба Б (обход `catch_unwind` в `catch_unit_panic` — `fn catch_unit_panic
{ f() }`):** 4-файловый батч (`pio_repro`/`pio2` — плохие носители, `pio5` —
хороший) вернул старый симптом обрыва: параллельные воркеры успели
напечатать свои строки, но **финальной сводки `===== SUMMARY =====` не
появилось**, процесс завершился без тally (в логе — `nova: internal error
... a scoped thread panicked` в самом конце). Восстановлено, `diff` против
бэкапа — идентично, повторный прогон батча — чистая сводка `PASS: 1
FAIL: 2`.

### 3. Регресс-фикстуры

- **Neg-фикстура (правило 5, новый код `E_CODEGEN_TYPE_UNKNOWN`):**
  `spec_tests/conformance/neg/m221_401_path_call_unimported_effect_neg.nv` —
  стартовый носитель брифа дословно (`Io.write_out("")` без импорта).
- Отдельной pos-фикстуры не заводил: `spec_tests/conformance/standalone/
  m221_401_timestamp_static_singlefile.nv` (создана окном p401 утром) уже
  пинит позитивную сторону ТОГО ЖЕ Path-call-канала (импорт есть →
  компилируется) для соседнего статик-метода — дублировать не стал, но
  ДОПОЛНИТЕЛЬНО прогнал `pio5.nv`-эквивалент (`Io.write_out("hi".bytes())`
  с `import std.io`) вручную через `nova build` → `.exe` → запуск → вывод
  `hi`, exit=0 (см. «Пробы выполнения» ниже) — полный сквозной прогон
  подтверждён, хоть и не оставлен фикстурой в дереве (эквивалентный канал
  уже покрыт существующей фикстурой №81/№401-timestamp).

### 4. Стартовый носитель

```
module pio
fn main() Io -> () { ro _a = Io.write_out("") }
```
`nova check` → `PASS: 1 FAIL: 0` (без изменений — чекер не тронут).
`nova build`/`nova test` теперь:
```
error: [E_CODEGEN_TYPE_UNKNOWN] Path call return type unknown for method=write_out
  --> pio_repro.nv:2:30
  hint: the type-checker did not annotate this expression's C type (compiler-conventions.md §0) — this is usually a missing `import` for the type/effect used on the left of `.`; if the import is already present, this is a compiler bug, please report it.
```
exit=1 (build) / отдельная `FAIL`-строка + корректная сводка батча (test).
Компилятор **не падает** — критерий приёмки соблюдён буквально.

### 5. `nova check std/src`

```
PASS: 151  FAIL: 26  WARN: 61
```
Канон 151/26/61 — **без изменений**.

### 6. Размер `emit_c.rs`

`64592` → `64681` строк, **+89** (новый хелпер `fatal_codegen_type_unknown` +
его doc-comment + расширенные call-сайты). Ratchet не поднимал — по
инструкции это решение интегратора.

### Целевые батчи (не мега-CU, точечно)

`nova test std/src/crypto std/src/identifiers std/src/io` →
`PASS: 8 FAIL: 2 SKIP: 11` — оба FAIL (`identifiers/uuid_test`,
`identifiers/uuid_namespace_test`) — задокументированный пред-существующий
CC-FAIL (список №320, не регрессия).

`nova test std/src/concurrency std/src/net std/src/time std/src/encoding` →
`PASS: 17 FAIL: 4 SKIP: 40` — все 4 FAIL (`encoding/serde/decode_errors_
test`, `net/addr`, `time/cron_test`, `time/civil/civil_arith_test`) — тот же
задокументированный пред-существующий список (PROGRESS-p401.md, таблица
«Прогон хвоста»), не регрессии. `concurrency/retry_test` — `PASS` (фикс
№402 держится). **`P67-LEGACY` не встретился НИ РАЗУ** ни в одном из
батчей; оба батча дошли до нормальной сводки (обрыва прогона не было даже
без специально подсунутых плохих носителей — что и требовалось).

## Изменённые/новые файлы

- `compiler-codegen/src/codegen/emit_c.rs` — 10 сайтов паники → честная
  диагностика через новый `fatal_codegen_type_unknown`.
- `compiler-codegen/src/test_runner.rs` — `catch_unit_panic` (pub),
  `catching_panic_active` (pub), thread-local флаг, обёртка вокруг
  `codegen_to_c`.
- `nova-cli/src/main.rs` — `run_catching` (внешний `catch_unwind`), хук
  тише при активном перехвате, `cmd_build` обёрнут `catch_unit_panic`.
- `spec_tests/conformance/neg/m221_401_path_call_unimported_effect_neg.nv`
  — новая neg-фикстура (правило 5).

## Не сделано / вне мандата

- Мега-CU `spec_tests/conformance` — авторитетный гейт, прогоняет
  интегратор (CPU-дисциплина брифа).
- Флагман-examples под `--strict-effects` — эта волна не behavior-changing
  для чекера (нулевой diff в `nova check std/src`), но формально не
  прогонялся — интегратор решает, нужен ли для этого класса правок.
- Глубинный checker-фикс самого гэпа №81/№401 (чтобы `Io.write_out(...)`
  РЕАЛЬНО компилировался без импорта) — сознательно не делался: тот же
  риск 42-файлового регресса, а бриф явно разрешил честную ошибку как
  приемлемый исход вместо этого.
