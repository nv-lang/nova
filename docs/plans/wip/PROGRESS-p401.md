<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# PROGRESS — окно p401-ci-red

Ветка `p401-ci-red`, worktree `d:/Sources/nv-lang/nova-p401`.

## №402 — CC-FAIL `retry_test` — ЗАКРЫТ

### Локализация

`std/src/concurrency/retry.nv:129-132`:
```nova
result ?? match last_error {
    Some(e) => throw e
    None    => throw last_error!!      // unreachable
}
```
Оба арма `match` — `throw`, т.е. диверджат (never-тип). Использовано как RHS `??`
на `Option[T]`. Специализация `T=int,E=int` (`RetryPolicy.execute[T,E]`,
`std/src/concurrency/retry.nv:102`) давала CC-FAIL:
`retry_test.c:11982:54: incompatible operand types ('nova_int' and 'nova_unit')`.

### Корень (два слоя, оба вскрыты и починены)

**Слой 1 (компиляция).** `emit_c.rs`'s `emit_match` для match, где ВСЕ армы
диверджат, не находит арма-источника типа (первый/второй pass явно
пропускают диверджащие армы) и откатывается на дефолт `nova_unit`. Канал
чекера (`infer_match_common_primitive`/`resolved_types[match_id]`) для этого
match МОЛЧИТ **корректно** — функция генерик (`T`, `E`), и тип `E`
(«unwrapped-inner от `last_error: Option[E]`») ещё не подставлен на этапе
чекера (gs-gate §0, проверено пробой: попытка докормить канал типом `a`
для этого случая была отклонена gs-гейтом, значит канал СОЗНАТЕЛЬНО не
место фикса для этой формы — тип известен только на mono-этапе кодогена).

Правильный канал для mono-специфичного факта — уже существующий механизм
`emit_expr_with_target_type`/`expr_diverges_125`/`emit_divergent_with_target_125`
(Plan 125, прецедент №118 — та же функция, тот же класс бага для `nova_unit`
target). Баг: guard `target_ty_c != "nova_int"` слепо исключал `nova_int`-target
из этого механизма — верно для ГОЛОГО диверджащего выражения (`throw e` уже
типизирован как `nova_int` в своём dummy), но НЕВЕРНО для `Match`, чей
`nova_unit`-дефолт остаётся неисправленным.

Фикс: `compiler-codegen/src/codegen/emit_c.rs`, `emit_expr_with_target_type`
(~32984) — guard сужен: `nova_int`-target больше не исключает `ExprKind::Match`
из `emit_divergent_with_target_125`. Маркер `[M-402-match-all-diverge-int-target]`.

**Слой 2 (рантайм, вскрыт ПОСЛЕ фикса слоя 1 — компиляция прошла, тесты упали).**
`Coalesce`(`??`) для `Option` RHS всегда строил C-тернарь
`(some_check ? opt.value : <right>)`. Когда `<right>` — `match`, его кодоген
(`emit_match`) — НЕ C-выражение, а последовательность STATEMENT'ов
(`self.line(...)`), вытолкнутых в поток БЕЗУСЛОВНО, ДО тернаря. Итог: армы
match'а исполнялись КАЖДЫЙ РАЗ, даже когда `opt_tmp` был `Some` и тернарь их
игнорировал. Оба regression-теста `retry_test.nv` поймали это:
- «первая попытка успешна» (T=int): `result=Some(42)`, `last_error=None` —
  None-арм всё равно исполнялся → `RuntimeNoneError` (`last_error!!` на `None`).
- «retry до успеха» (T=str): `result=Some("success")` на 3-й попытке,
  `last_error=Some("transient")` от 2-й — Some-арм всё равно исполнялся →
  `throw "transient"` вместо возврата "success".

Это ПРЕД-СУЩЕСТВУЮЩИЙ баг (не внесён фиксом слоя 1) — T=str специализация уже
до слоя-1-фикса шла через `emit_divergent_with_target_125` (target≠nova_int) и
несла тот же баг; retry_test.nv просто НИКОГДА не компилировался целиком до
слоя-1-фикса, поэтому баг слоя 2 никогда не запускался ни разу.

Фикс: `compiler-codegen/src/codegen/emit_c.rs`, `ExprKind::Coalesce` / Option-
ветка (~36038) — при `right` формы `ExprKind::Match` кодоген переключается с
тернаря на `if/else` с общим result-temp: RHS-statement'ы (армы match'а)
эмитятся ВНУТРИ `else`-ветки, исполняются только когда `opt_tmp` реально
`None`. Любая другая форма RHS (литерал/вызов/panic-comma-expr) остаётся на
прежнем тернарном пути байт-в-байт. Маркер `[M-402-coalesce-match-eager-side-effect]`.
Result-ветка `??` (`Result ?? fb`) той же природы БЫ страдала для RHS=match,
но живых носителей в кодовой базе не найдено (грепнуто `?? match` по всему
дереву — единственное вхождение это retry.nv) — Result-ветка НЕ трогалась
(вне мандата этой волны, но зафиксировано как известный смежный риск).

### Пробы «подсунь заведомо негодное»

**Проба слоя 1** (откат guard-сужения — вернул `target_ty_c != "nova_int"`
без исключения для `Match`): CC-FAIL возвращается дословно
`retry_test.c:11982:54: error: incompatible operand types ('nova_int'
(aka 'long long') and 'nova_unit')`. Подтверждено.

**Проба слоя 2** (откат if/else-ветки — вернул тернарь для RHS=Match без
исключения): компилируется (слой 1 всё ещё чинит тип), но тесты падают
рантаймом:
`RUN-FAIL std/src/concurrency/retry_test # FAIL: execute: первая попытка
успешна — RuntimeNoneError | FAIL: execute: retry до успеха — transient`.
Подтверждено — оба вердикта дословны, оба воспроизведены прогоном.

### Позитив / негатив

- Позитив: `nova test std/src/concurrency/retry_test.nv` → `PASS: 1 FAIL: 0`.
- Негатив (проба «сломай свой фикс», см. выше) — оба слоя дают ТОЧНЫЙ прежний
  симптом при откате: слой 1 → CC-FAIL с тем же текстом; слой 2 → RUN-FAIL с
  теми же двумя сообщениями.
- Полный батч `nova test std/src/concurrency` → `PASS: 4 FAIL: 0 SKIP: 5`
  (SKIP — модули без test-блоков, компилируются штатно).
- Блаcт-радиус: `grep -rln '?? match' std/src examples spec_tests` →
  единственное вхождение `std/src/concurrency/retry.nv` (сам носитель бага).

### Регресс-фикстура

`std/src/concurrency/retry_test.nv` (уже существующий тест, БЕЗ изменений) —
теперь реально ловит регресс (раньше падал компиляцией, значит НИКОГДА не
запускался; правило 1 test-conventions.md соблюдено — фикстура УЖЕ существует
рядом с модулем, никакой новый файл не нужен).

---

## №401 — INTERNAL ERROR на `.now()` через Path-вызов — ЗАКРЫТ

### Локализация

Батчи std/src по подкаталогам (crypto → identifiers → …) не понадобились —
крашится ВНУТРИ каталога `crypto` ещё до того, как дойти до алфавитного
конца: `nova test std/src/crypto` вылетает после `SKIP hmac` (module-файлы
без test-блоков) с тем же `[P67-LEGACY]`. Изоляция до одного файла:
`nova test std/src/crypto/jwt.nv` (СТЕНДЭЛОУН, без соседей) — крашится тем
же текстом. `nova test std/src/crypto/hmac_test.nv` — PASS сам по себе (не
виновник по имени, вопреки заголовку записи №401 — «сразу после
hmac_test» относится к порядку прогона CI, не к содержимому hmac_test.nv).

Строка-виновник: `std/src/crypto/jwt.nv:112`
`ro now_ms = Timestamp.now().unix_millis() as u64` — `Jwt.validate_hs256`.
`jwt.nv` НЕ импортирует `std.time.duration` (модуль, где объявлен
`Timestamp`) — ни явно, ни через файл-сосед (`module crypto.jwt` — файл
сам себе модуль, папка `crypto/` не co-equal группа).

### Корень

Грепом по всему `std/src` (`Timestamp\.now\|Monotonic\.now` без
соответствующего `import std.time.duration` в том же модуле) найдено ЕЩЁ
ТРИ файла с той же дырой: `std/src/identifiers/{snowflake,ulid,uuid}.nv`.
Все четыре крашатся standalone тем же `[P67-LEGACY]` текстом (проверено
индивидуально до фикса).

Это НЕ новый класс бага — это ТОТ ЖЕ класс, что реестр 221.1 уже закрывал
под меткой №81 (`spec_tests/conformance/standalone/
m221_81_monotonic_static_singlefile.nv`, коммент там расписывает механику
дословно). Чекер (`f1_expr`, `types/mod.rs:14625` — `ExprKind::Path(parts)
if parts.len()==2`) резолвит `Type.method(...)` через
`self.method_overloads(parts[0], parts[1])` — ГЛОБАЛЬНЫЙ, не
module-scoped реестр (`self.sig`), поэтому НАЗВАНИЕ резолвится и БЕЗ
импорта (отсюда `nova check` зелёный — ложно-успокаивающий сигнал). Но
если `method_overloads` возвращает `None` (типично, когда `Timestamp`/
`Monotonic` реально НЕ импортирован и `sig`-таблица тоже не содержит его в
данном single-file/folder compile-unit — сам факт, что глобальный `sig`
иногда ВСЁ РАВНО находит его вне зависимости от импорта, зависит от того,
что ещё попало в ТЕКУЩИЙ compile-unit), ветка тихо делает `return` — ни
диагностики, ни записи в `resolved_callees`/`resolved_types`. Молчание
чекера доезжает до `emit_c.rs`'s `infer_call_ret_c` (легаси-фоллбэк для
Path-вызовов, ~59492), которая перебирает ВСЕ известные builtin/эффект/
sum-вариант источники типа возврата — ни один не подходит для
пользовательского `Timestamp.now`, - и падает в `panic!` (~59633).

**Почему `nova check` зелёный, а не одна из проверок ловит отсутствие
импорта:** аудит №81 (комментарий `types/mod.rs:14755-14777`,
`compiler-codegen/src/types/mod.rs`) документирует, что ОБЩИЙ
checker-диагноз «Type not declared/imported» для ЭТОЙ ветки был
СПРОБОВАН И ОТКАЧЕН — регрессировал `nova check std/src` на 42 файла
ложными позитивами (cross-module top-level `const`-ресиверы вроде
`I64_MIN.to_nanos()`, чисто-рантайм intrinsic-неймспейсы без `.nv`
`type`-декларации вообще — `ChanReader`/`Channel`/`CancelToken`/
`StringBuilder`/… — резолвятся ТОЛЬКО через emit_c-шный хардкод-диспатч,
чекеру не видны). Настоящий фикс, применённый там же (окно №81) и
применённый здесь — добавить недостающий импорт в файл-виновник.

### Фикс

`std/src/crypto/jwt.nv`, `std/src/identifiers/snowflake.nv`,
`std/src/identifiers/ulid.nv`, `std/src/identifiers/uuid.nv` — добавлена
строка `import std.time.duration` (модуль, где живёт `export fn
Timestamp.now() Time -> Timestamp` в `std/src/time/duration/timestamp.nv`).
Канонический импорт-паттерн, использованный ИДЕНТИЧНО в других
cross-module потребителях `Timestamp.now()`/`Monotonic.now()`
(`std/src/concurrency/supervised_deadline_test.nv:34`,
`std/src/net/supervised_cancel_accept_test.nv:59`,
`std/src/testing/handlers/core.nv:92`).

**Это НЕ фикс чекер-канала, вопреки формулировке в задании.** Бриф
предполагал (по тексту паники), что чекер должен «доанннотировать»
Path-вызов, — разумная гипотеза по симптому. Разбор показал: ЭТО ТОЧНО ТОТ
ЖЕ класс, что реестр УЖЕ закрывал под №81, и там ЖЕ документирован (в
исходнике чекера, не только в реестре) провал попытки закрыть класс
ОБЩИМ checker-фиксом — откат на 42 файла ложных позитивов. Повторять
проваленный подход означало бы либо повторить регресс, либо сузить фикс
ДО «тот же самый частный случай, что уже §решён» — что и есть добавление
недостающего импорта. Решение задокументировано ЗДЕСЬ явно, чтобы
приёмка видела: отклонение от буквы брифа — по прецеденту В ТОМ ЖЕ
файле компилятора, не произвольное.

### Проба «подсунь заведомо негодное»

Убран `import std.time.duration` из `jwt.nv` (единственная строка) →
`nova test std/src/crypto/jwt.nv` даёт ДОСЛОВНО:
`nova: internal error at …/emit_c.rs:59633: [P67-LEGACY] Path call return
type unknown for method=now — checker must annotate
(compiler-conventions.md §0)`. Подтверждено, импорт восстановлен.

### Регресс-фикстура

`spec_tests/conformance/standalone/m221_401_timestamp_static_singlefile.nv`
— зеркало `m221_81_monotonic_static_singlefile.nv` для `Timestamp.now()`
конкретно (тот же Path-call-канал, соседний статик-метод). `EXPECT_STDOUT
M221_401_TIMESTAMP_STATIC_SINGLEFILE_OK`. `nova test
spec_tests/conformance/standalone/m221_401_timestamp_static_singlefile.nv`
→ `PASS: 1 FAIL: 0`.

### Прогон «хвоста» — что вскрылось после снятия блокера

Полный проход по ВСЕМ подкаталогам `std/src` (батчами, см. команды выше):
concurrency, crypto, identifiers, encoding, fs, io, math, net, os, path,
runtime, testing, text, time, unicode, data, checksums, ffi, collections,
prelude, _experimental — **P67-LEGACY больше НИГДЕ не встречается.**

Вскрылось 5 ДОПОЛНИТЕЛЬНЫХ красных, ранее скрытых обрывом прогона на
crypto. Каждый сверен с независимо задокументированным δ0-baseline
(`docs/plans/221.1-bug-sweep.md` №320, окно p320-indexmap, 2026-08-04:
«все пред-существующие CC-FAIL/ICE — concurrency/retry_test,
encoding/serde/decode_errors_test, net/addr, time/cron_test,
time/civil/civil_arith_test, identifiers/\*, crypto/\* — воспроизведены
НЕИЗМЕНЁННЫМИ на main, не регрессии») — ВСЕ пять входят в этот список,
НИ ОДИН не новый:

| Файл | Класс | Пред-существующий маркер |
|---|---|---|
| `encoding/serde/decode_errors_test` | CC-FAIL | `[M-serde-decode-errors-option-vec-ctype-mismatch]`, backlog-followups.md, OPEN 2026-07-31, P2 |
| `identifiers/uuid_test` | CC-FAIL | в списке №320 (`identifiers/*`) |
| `identifiers/uuid_namespace_test` | CC-FAIL | в списке №320 (`identifiers/*`) |
| `net/addr` | CC-FAIL | в списке №320 (`net/addr`) дословно |
| `time/cron_test` | CC-FAIL | в списке №320 (`time/cron_test`) дословно |
| `time/civil/civil_arith_test` | RUN-FAIL (overflow) | в списке №320 (`time/civil/civil_arith_test`) дословно |

`concurrency/retry_test` — тоже в этом списке (пред-существующим на
2026-08-04, ДО этой волны) — теперь ЗАКРЫТ (№402, см. выше).

**Вывод:** хвост прогона расчищен от НОВОГО красного. Шесть находок выше —
известный, отдельно поднадзорный backlog (P2/К4), НЕ блокеры ЭТОЙ волны,
чинить не входит в мандат №401/№402.

---

## Итог волны

- №401 — ЗАКРЫТ (4 std-файла + import; регресс-фикстура в
  spec_tests/conformance/standalone/).
- №402 — ЗАКРЫТ (emit_c.rs, два слоя; регресс — существующий
  retry_test.nv, без изменений).
- `cargo build --release --manifest-path nova-cli/Cargo.toml` — чист (только
  pre-existing warnings, без ошибок).
- Полный обход `std/src` по подкаталогам — ни одного НОВОГО красного;
  P67-LEGACY не воспроизводится нигде.
- Изменённые файлы: `compiler-codegen/src/codegen/emit_c.rs`,
  `std/src/crypto/jwt.nv`, `std/src/identifiers/{snowflake,ulid,uuid}.nv`,
  `spec_tests/conformance/standalone/m221_401_timestamp_static_singlefile.nv`.
- Не сделано (вне мандата): мега-CU `spec_tests/conformance` (авторитетный
  гейт — прогоняет интегратор), флагман-examples под `--strict-effects`.
