# PROGRESS — окно p428-effect-enforce-property

Модель: **sonnet**. Ветка `p428-effect-enforce-property`, worktree
`d:/Sources/nv-lang/nova-p428`.

## 1. Дефект (кратко)

`E_BANG_REQUIRES_FAIL` (Plan 221.1 №113, D62/D85) проверял **форму**:
буквальный `!!`-токен в СИНТАКСИСЕ тела САМОЙ `export fn`. Не смотрел на
эффекты вызываемых функций вовсе — ни на один hop. Два вскрытых пробела:

- Проба 1 (интегратор): прямой `!!` в export-теле — энфорс работал.
- Проба 2 (интегратор): `!!` спрятан в приватном хелпере трёх уровней
  глубины (`level3` → `level2` → `level1` → `export fn api`) — `nova
  check` PASS молча.
- Найдено этим окном ДОПОЛНИТЕЛЬНО (probe3): даже ОДИН hop без `!!`/`?`
  вовсе — `export fn api() -> int => declared_fail(x)`, где
  `declared_fail` явно несёт `Fail[str]` в своей сигнатуре — ТОЖЕ PASS'ил
  молча. Старый чекер НИКОГДА не читал `callee.effects`.

## 2. Фикс

Новый файл `compiler-codegen/src/types/fail_reach.rs` — отдельный,
дополнительный проход `check_module_impl` (`types/mod.rs`), запускается
**после** финализации `resolved_callees` (§0/196 чекер-канал, та же
точка, что `fiber_safety::run`) — той же архитектуры, что Plan 238.

- Вычисляет least fixed point `requires_fail(F)` над ЛОКАЛЬНЫМ графом
  вызовов (`Item::Fn` в `module.items`): fn требует `Fail`, если (а) её
  СОБСТВЕННАЯ сигнатура уже несёт `Fail[…]`, ЛИБО (б) её тело — включая
  вложенные closure/handler-литерал-опы, идущие INLINE (capability
  окружающей fn, не своё собственное) — содержит недогашенный `!!`, ЛИБО
  (в) зовёт (транзитивно, любая глубина, через `resolved_callees`
  fully-resolved — методы/generic-dispatch включены) другую локальную fn
  с `requires_fail` уже true, ЕСЛИ этот конкретный вызов не обёрнут
  локальным `with Fail = …`.
- Для `export fn`: если `requires_fail(fn)` и `Fail` не в её сигнатуре →
  `[E_BANG_REQUIRES_FAIL]`. Скоуп (только export, приватные под D28
  auto-inference) — **не тронут**, ровно как утверждала политика брифа.
- Старый `check_bang_requires_fail_block`/`_expr` (синтаксис-only,
  `types/mod.rs`) — **удалён**, заменён целиком.
- `check_defer_bodies` (D158) имел ТУ ЖЕ дыру для `defer`/`errdefer`:
  собственный one-hop `fn_effects`-lookup не видел транзитивный `Fail`.
  Закрыт тем же fixed point'ом — `fail_reach::requires_fail_by_name`,
  ИМЯ-based вариант (тот же generic-по-K walker, ключ `String` вместо
  `Span` — эта проверка бежит РАНЬШЕ в пайплайне, до готовности
  `resolved_callees`, резолвит вызовы тем же способом, что уже
  использует существующий `call_target_name`/`fn_effects`).
- Побочный найденный баг: `has_throw_in_expr`'s `ExprKind::Bang` arm
  (`types/mod.rs`, использует `infer_effects`/D28) считал throw-источником
  только `inner`, НЕ сам оператор `!!` — `risky(x)!!` не засчитывался
  throw'ом вовсе (в отличие от соседнего `Throw(_) => true`). Однострочный
  фикс (`Bang(_) => true`, тот же D85-принцип «`!!` всегда throw-стиль»).

## 3. Матрица (позитив/негатив на каждую клетку)

Все фикстуры прогнаны через `nova check` собранным release-бинарём.

| Клетка | Позитив (`Fail` обязан требоваться) | Негатив (структурно невозможен / легально discharge) |
|---|---|---|
| Прямое тело | `probe1.nv` — FAIL (E_BANG_REQUIRES_FAIL) ✅ | `probe2_fixed.nv` (Fail объявлен) — PASS ✅ |
| Цепочка приватных (любая глубина) | `probe2.nv` (3 hops), `spec_tests/…/neg/d85_bang_requires_fail_transitive_neg.nv` — FAIL ✅ | `d85_bang_requires_fail_pos.nv`: `ok_transitive_chain_declared` (Fail объявлен один раз на границе) — PASS ✅ |
| Замыкание | `closure_pos.nv` (`!!` внутри `|| …`, вызван синхронно) — FAIL ✅ | `closure_neg.nv` (чистое замыкание) — PASS ✅; `d85_bang_requires_fail_pos.nv`: `ok_closure_bang_declared` — PASS ✅ |
| Вызов метода | `method_pos.nv` (`Type.method`, приватный, с `!!`) — FAIL ✅ | `method_neg.nv` — PASS ✅; `d85_bang_requires_fail_pos.nv`: `ok_method_chain_declared` — PASS ✅ |
| Обобщённый вызов | `generic_pos.nv` (`fn[T](v T)`, приватная, с `!!`) — FAIL ✅ | `generic_neg.nv` — PASS ✅ |
| `defer`/`@cleanup` | `defer_pos.nv` (enclosing БЕЗ Fail, defer зовёт транзитивную цепочку) — FAIL (D158-defer-fail-not-in-sig) ✅ | `defer_neg.nv` (чистый helper) — PASS ✅ |
| Обработчик эффекта | `handler_pos.nv` (`!!` ВНУТРИ тела опа handler-литерала, `with SomeOther = effect SomeOther { op(...) { risky(x)!! } }`) — FAIL ✅ | `probe2_handler.nv` (транзитивная цепочка целиком обёрнута `with Fail = effect Fail {...}`) — PASS ✅; `d85_bang_requires_fail_pos.nv`: `ok_transitive_chain_with_handler`, `ok_local_with_fail` — PASS ✅ |
| Прямой вызов explicitly-Fail callee, без `!!`/`?` | `probe3.nv` — FAIL ✅ | — |

Вердикт: во всех клетках `Fail` требуется корректно; там, где отказ
структурно невозможен (чистые функции/замыкания без throw где-либо в
достижимом коде) — ложного требования нет.

## 4. Проба «подсунь заведомо негодное»

1. Обход выносом в хелпер (проба 2 из брифа) — **красный**:
   `probe2.nv` → `[E_BANG_REQUIRES_FAIL] call to \`level1\` requires
   effect \`Fail[…]\`…` (span на call-сайт `level1(x)` в `api`).
2. Сабботаж (`fail_reach::run` вызов закомментирован, ребилд, прогон,
   восстановлено, ребилд):
   - `probe1.nv` (прямой `!!`) — **PASS молча** (0 диагностик).
   - `probe2.nv` (транзитивная цепочка) — **PASS молча** (0 диагностик).
   - После восстановления + ребилда — оба снова FAIL с прежними
     сообщениями (точная проверка `diff` до/после сабботажа —
     идентичный текст ошибки).

Оба вердикта дословно воспроизведены выше (см. §7 «Транскрипты»).

## 5. Таблица ревизии прочих энфорсов эффектов

| Энфорс | Форма или свойство | Нужен ли фикс |
|---|---|---|
| `E_BANG_REQUIRES_FAIL` (№113/D85, Fail) | Была ФОРМА | **Пофикшено этим окном (№428)** |
| `E_RAW_EFFECT_OP_UNDECLARED` (№131, raw `Effect.op(...)`) | ФОРМА — подтверждено пробой (`rawop_chain.nv`: export через 3 приватных hop'а до `Log1.info(...)`, PASS молча). Собственный doc-комментарий чекера прямо заявляет «Scope mirrors E_BANG_REQUIRES_FAIL (№113) exactly» — тот же класс, тот же корень. | **Нужен, НЕ пофикшено этим окном** (обобщение архитектуры `fail_reach.rs` на произвольное имя эффекта, не только `Fail` — по объёму сравнимо с основным фиксом; оставлено отдельным follow-up, номер TBD) |
| `E_UNDECLARED_TRANSITIVE_EFFECT` (`--strict-effects`, D62 §Правило 1, Net/Db/Log/… кроме Fail) | СВОЙСТВО (читает `callee.effects`, property-based по конструкции) — НО та же неполнота данных: `callee.effects` для приватных fn НЕ транзитивно выведены (`infer_effects` — single-pass, не fixed-point через цепочку приватных вызовов), та же корневая причина ordering, что чинил №428 для Fail. Это НЕ «форма вместо свойства» — корректный property-read по СТАЛЫМ данным. | Смежный, но ОТДЕЛЬНЫЙ дефект — не в объёме «форма vs свойство» этого окна; экспериментальный флаг, ниже приоритет. Задокументировано, не чинено. |
| `E_DETACH_REQUIRES_EFFECT` (D50, `detach { }`) | СВОЙСТВО — безусловен для ЛЮБОЙ fn (приватной ИЛИ export), несущей `detach{}` буквально в теле (`state.is_export` НЕ гейтит этот энфорс вовсе — подтверждено пробой `detach_chain.nv`: приватная `level3` с `detach{}` флагуется НА МЕСТЕ, до всякой транзитивности). Обхода через приватный helper нет структурно — сам helper обязан объявить `Detach` или обернуть `with`. | Фикс не нужен |
| `check_forbid_intersection`/realtime-suspend/blocking-body checks (`check_callee_effects`) | СВОЙСТВО — читает `callee.effects` на КАЖДОМ hop (один уровень), безусловно (не за флагом), той же архитектуры что `check_transitive_effect_strict`. Та же потенциальная неполнота приватных эффектов, что у `E_UNDECLARED_TRANSITIVE_EFFECT` (не проверено эмпирически этим окном — вне бюджета). | Не проверено до конца — кандидат для отдельной ревизии, не входит в объём «форма vs свойство» (уже property-based по архитектуре) |
| «cancel-throw вне области» (§11, упомянуто в брифе) | Не найден отдельный чекер с этим точным названием — ближайшие кандидаты, D90/D158 exit-control в `check_defer_body_inner` (`Interrupt`/`Spawn`/`Supervised`/`Detach`/`Blocking`/`ParallelFor` внутри defer body — AST-level ban) и `check_try_return_only_*` (`?`-scope) — оба СИНТАКСИЧЕСКИ ограничивают КОНКРЕТНЫЙ узел (не транзитивны по конструкции — throw/spawn ВСЕГДА текстуально виден в самом defer body, обхода через helper нет, т.к. helper — ОТДЕЛЬНАЯ fn с собственным телом, D90 не запрещает ВЫЗОВ такого helper'а из defer body вовсе, только ЭТИ КОНКРЕТНЫЕ конструкции буквально внутри). Не нашёл более точного попадания под «§11» за отведённый бюджет — если это отсылка к другому месту, прошу интегратора уточнить путь. | Не идентифицирован однозначно — см. примечание |

## 6. Находки в `std`/`examples` (НЕ исправлено, НЕ ослаблено)

`nova check std/src`: baseline (фикс выключен, сабботаж) — **PASS 151 /
FAIL 26 / WARN 61** (совпадает с каноном из брифа). С фиксом — **PASS 146
/ FAIL 31 / WARN 54**. Diff — ровно **5 новых FAIL**, все — ОДИН корень:

- `std/src/encoding/json.nv` — `Lexer @char_at` (приватный метод,
  строка 458): `@input[p..].chars().next()!!` — unwrap на `Option[char]`,
  предполагается всегда-Some по инварианту (`p` всегда в границах), но
  ТИП этого не гарантирует. Транзитивно достижим из `Json.parse`/
  `JsonValue.try_from` (**exported**, публичный API std) через
  `Parser.new` → `Lexer.next_token` → `char_at`, ни один уровень не несёт
  `Fail`.
- `std/src/encoding/json_test.nv`, `std/src/encoding/serde/
  decode_errors_test.nv`, `std/src/encoding/serde/json.nv` (через
  `serde_neg`/`decode_errors_test` peer-merge) — тот же корень,
  транзитивно через `Json.parse`.
- `std/src/crypto/jwt.nv`, `std/src/crypto/jwt_test.nv` — `Jwt.decode_hs256`
  зовёт `Json.parse` тоже без `Fail` на своей границе.

`nova check examples`: baseline — FAIL 2 (`orm_demo.nv` — несвязанный
`Hash`-bound; `tls/echo_server.nv` — несвязанный `undefined identifier
session`). С фиксом — **3 новых FAIL**, ТОТ ЖЕ корень:
`examples/flagship/aggregator/src/main.nv`,
`.../src/api/report_json.nv`, `.../regressions/serde_encode_pointer_op/
serde_encode_pointer_op.nv` — все через `nova-polaris`'s `extract.nv`
(`JsonValue.try_from`) / `auth.nv` (`Jwt.decode_hs256`).

**Важно (риск для мега-CU гейта):** `spec_tests/conformance/` — ОДИН
folder-module (все файлы делят `module spec_tests.conformance`) —
проверено: `nova check` даже на файл, НЕ связанный с JSON
(`d85_bang_requires_fail_pos.nv`), уже подтягивает ЭТУ ЖЕ находку через
peer-merge (какой-то другой файл папки трогает `Json`/`Jwt`). Это
означает: `spec_tests/conformance` мега-CU (авторитетный гейт) С ВЫСОКОЙ
ВЕРОЯТНОСТЬЮ покраснеет из-за ЭТОЙ находки при слиянии, если `std`'s
`Json`/`Jwt`-цепочка не получит `Fail` (или `char_at`'s unsafe unwrap не
исправлен) ДО или ВМЕСТЕ со слиянием. **Не прогонял мега-CU сам**
(CPU-дисциплина — авторитетный гейт владельца); рекомендую прогнать его
ПЕРЕД решением о пуше.

Природа находки — ОДИН корень (`Lexer.char_at`), радиус — широкий (через
folder-module/prelude-подобный transitive-import). Правка публичного API
`Json`/`Jwt`/`std`-код — **отдельная волна**, не в объёме этого окна
(задача — чекер, не `std`). **Не правил молча, не ослаблял проверку.**

## 7. Транскрипты (ключевые)

```
$ nova check probe1.nv     # прямой !! — было и осталось FAIL
[FAIL] E_BANG_REQUIRES_FAIL at probe1.nv:9:12

$ nova check probe2.nv     # транзитивная цепочка — БЫЛО PASS, теперь FAIL
[FAIL] E_BANG_REQUIRES_FAIL call to `level1` requires effect `Fail[…]`… at probe2.nv:14:32

$ nova check probe3.nv     # прямой call без !!/? на explicitly-Fail callee — БЫЛО PASS, теперь FAIL
[FAIL] E_BANG_REQUIRES_FAIL call to `declared_fail`… at probe3.nv:13:33

# сабботаж (fail_reach::run закомментирован, ребилд):
$ nova check probe1.nv  → PASS (0 diagnostics)
$ nova check probe2.nv  → PASS (0 diagnostics)

# восстановлено, ребилд:
$ nova check probe1.nv  → FAIL (идентичный текст)
$ nova check probe2.nv  → FAIL (идентичный текст)
```

## 8. Спек-амендмент

`spec/decisions/04-effects.md`, D62 (в блоке `## D62`, сразу после
существующего №131-амендмента, перед `## D65`) — новый `> ✅ ENFORCED
(Plan 221.1 п.11, №428, 2026-08-07)` блок, тот же формат, что №131.
Уточняет §Правило 2 («Fail транзитивен через границы вызовов») как
**резолвленное свойство callee**, не текстовый поиск `!!` в теле самой
`export fn`; документирует механику (`fail_reach.rs`, fixed point,
scope-неизменность приватной границы), находку в `std`, и таблицу
ревизии кратким списком.

## 9. Дока в `.nv`/диагностики

Все новые/изменённые diag-строки и doc-комментарии в `.nv`-фикстурах —
английский (см. `feedback-nv-doc-comments-english.md`); внутренние `//`
в `.rs` — как было (существующий стиль файла — смешанный RU/EN, не
нарушаю конвенцию, следую локальному соглашению `types/mod.rs`).

## 10. Изменённые файлы

- `compiler-codegen/src/types/fail_reach.rs` — **новый**, основной фикс.
- `compiler-codegen/src/types/mod.rs` — регистрация модуля, удаление
  старого `check_bang_requires_fail_block`/`_expr` + вызова в `check_fn`,
  подключение `fail_reach::run` в `check_module_impl`, подключение
  `fail_reach::requires_fail_by_name` в `check_defer_bodies`, `pub(crate)`
  на `has_fail_effect`/`call_target_name`, однострочный фикс
  `has_throw_in_expr`'s `Bang` arm.
- `spec/decisions/04-effects.md` — D62-амендмент (см. §8).
- `spec_tests/conformance/neg/d85_bang_requires_fail_transitive_neg.nv` —
  **новый** neg-фикстура (правило 5, страж `check-test-fixture-coverage.sh`
  доволен — код не новый, но forma-класс покрыт explicitly).
- `spec_tests/conformance/d85_bang_requires_fail_pos.nv` — расширен
  регресс-гардом на новые легальные формы (транзитивная цепочка с
  декларацией/with, метод, замыкание).

## 11. Незакрытое / решения нужны от владельца

1. **Блокер потенциально для мега-CU-гейта** — `std`'s `Json`/`Jwt`
   публичный API не декларирует `Fail`, хотя транзитивно может throw
   (см. §6). Нужно решение: чинить `std` в этой же волне (отдельным
   коммитом) ИЛИ временно допустить регресс с последующим фиксом ИЛИ
   другое.
2. `E_RAW_EFFECT_OP_UNDECLARED` (№131) — подтверждён ТОТ ЖЕ дефект,
   требует отдельного окна (обобщение `fail_reach.rs` на произвольные
   имена эффектов).
3. «cancel-throw вне области» (§11) — не идентифицирован однозначно,
   нужна ссылка от интегратора.
