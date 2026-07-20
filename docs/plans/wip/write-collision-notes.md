# [M-fmt-write-protocol-collision-cycle-adjacent] — чекпоинт

Worktree: `nova-wpcol`, ветка `p-fix-write-collision` (main @ c190de41e). Модель: sonnet.

## Окружение (см. project-worktree-nova-test-setup.md)

- Свой релизный бинарь собран: `compiler-codegen/target/release/nova-codegen.exe`
  (cargo build --release, ~1m) и `nova-cli/target/release/nova.exe` (~3m35s).
- libuv скопирован из main (`.git` убран), `target/libuv-cache/libuv.lib` скопирован.
- env: `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → main repo vcpkg_installed.
- `nova-codegen test-build <file>` требует `--rt-dir`/`--cg-include` на main repo
  compiler-codegen (worktree сам их не имеет — vcpkg тяжёлый, exFAT без junction).

## КРИТИЧЕСКАЯ НАХОДКА (не моя зона, БЛОКИРУЕТ folder-CU гейт целиком)

`spec_tests/conformance` folder-CU (`nova test --positive --compile-error`,
`nova-codegen test-build` на любом файле папки) падает с ICE, НЕ доходя до
моего репро вообще:

```
nova: internal error ...emit_c.rs:53362: [P67-LEGACY] method call `.write_at`
return type unknown — checker must annotate; obj=Ident(q) ... file_id: 188
```

Источник — `spec_tests/conformance/d216_ptr_methods_174_5.nv:18`
(`q.write_at(1, 99)`, Plan 174.5, старая стабильная фикстура). ПОДТВЕРЖДЕНО на
НЕТРОНУТОМ (baseline, до моих правок) дереве worktree — т.е. это ПРЕДСУЩЕСТВУЮЩИЙ
дефект main @ c190de41e, НЕ вызван мной. Похоже на регресс из соседней текущей
работы (checker return-type annotation gap — зона closure-peek/check_consume,
явно в моём брифе как "НЕ трогай"). Внёс в отчёт владельцу, чинить НЕ буду (вне
скоупа, чужая зона).

**Обход верификации**: `nova check <path>` (type-checker ONLY, не доходит до
emit_c/C-compile) — E7301 это ЧЕКЕР-диагностика (`assignable`/
`protocol_mismatch_found`, types/mod.rs), значит `nova check` полностью
достаточен для гейта ЭТОЙ волны и не задевает write_at ICE (другой pipeline stage).
`nova check <file>` — судя по всему, чекает файл ИЗОЛИРОВАННО (unused-import
warnings для имён, нужных ТОЛЬКО другим peer-файлам той же папки) — не 100%
уверен, что это даёт полную folder-CU семантику для cross-file резолва; нужно
перепроверить на `nova check <dir>` (recursive walk).

Также отдельно: `nova-codegen test-build` на ЛЮБОМ изолированном single-file
repro (own module, вне spec_tests.conformance) с `FmtCtx.bare`/`Display`-
диспетчем даёт СВОЙ отдельный CC-FAIL (`passing 'int' to nova_str`,
bool/char `@display` primitive bodies, generated C ~8826/8835/8844) —
ТОЖЕ предсуществующий, ТОЖЕ не мой, ТОЖЕ emit_c-стадия. Значит full-pipeline
(test-build/nova test) verification для этой волны сейчас НЕДОСТУПЕН в этом
дереве ни через folder, ни через изолированный файл — использую `nova check`.

## Корень (подтверждено чтением кода, НЕ гипотеза)

`compiler-codegen/src/types/mod.rs`, `TypeCheckCtx::build` (~3603-3657):
`types: HashMap<String, &'a TypeDecl>` — ГЛОБАЛЬНАЯ по ГОЛОМУ имени типа
(НЕ module-qualified). Цикл `for item in &module.items { Item::Type(td) =>
types.insert(td.name.clone(), td) }` — LAST WINS, без учёта, из какого
модуля/через какой import-путь пришёл каждый td.

Комментарий на этом же месте (~3610-3618) уже документирует ИДЕНТИЧНЫЙ класс
бага для `ErrorKind` (http/io/compress все объявляют `ErrorKind`) — но
воркэраунд там (`sum_variant_names` side-table) покрывает только sum-variant-
имена, НЕ покрывает `types`-мапу, используемую `protocol_mismatch_found`
(~14510: `let td = self.types.get(name)?;`) для СТРУКТУРНОЙ проверки
protocol-satisfaction (`fn f(sink Write)` — резолвит `Write` через ЭТУ же
глобальную мапу).

`std.io.core` (std/src/io/core.nv:51) И `std.prelude.protocols`
(std/src/prelude/protocols.nv:235) ОБА объявляют `export type Write protocol`
— РАЗНЫЕ протоколы (io.Write требует `flush()`, fmt-Write — только
`@write(bytes)`). Какой из двух побеждает в `self.types["Write"]` — зависит
от ПОРЯДКА `module.items` (порядок мержа transitive-import-графа CU,
imports.rs), который меняется при добавлении цикла fmt_buf↔protocols.

Открытый вопрос (СЛЕДУЮЩИЙ шаг): подтвердить эмпирически (инструментация
eprintln на `types.insert` при `td.name=="Write"`, file_id/span), что именно
`std.io.Write` реально присутствует в CU `d374`'s transitive-графе (никакой
conformance-файл explicit НЕ импортирует `std.io` — прямых импортов нет ни в
prelude/*, ни в fmt_buf/string_builder/raw_mem/protocols). Механизм ПОКА не
100% замкнут — граф, откуда io.Write попадает в CU именно при цикле, не
проверен инструментацией (следующий шаг после этого чекпоинта).

## План фикса (пока НЕ реализован)

Задача просит "идемпотентность регистрации по module_key" — но находка
показывает НЕ дублирование ОДНОГО протокола, а КОЛЛИЗИЮ ИМЁН двух РАЗНЫХ
протоколов в global-by-bare-name `types` map. Нужно решить: (а) квалифицировать
`types`-ключ по module_key ТОЛЬКО для протокол-резолва в
`protocol_mismatch_found`/`assignable` (узкий фикс, не трогая остальные
consumers `self.types`), или (б) как и `sum_variant_names`, завести parallel
lookup, разрешающий коллизию по видимости импорта в файле USE-сайта.
Ещё не выбрано — следующий шаг.

## Обновление (репро подтверждено RED через nova check + test-build)

- Ввёл минимальный репро: `std/src/runtime/fmt_buf/core.nv` — добавлена строка
  `import std.prelude.protocols.{Fmt}` сразу после `module runtime.fmt_buf`
  (протоколы.nv уже импортирует fmt_buf для Align/Sign/FmtKind → цикл).
  Помечено TEMP REPRO с явным маркером, НЕ фикс.
- `nova check spec_tests/conformance/d374_write_sink_decouple.nv` (проверяет
  ВЕСЬ folder-module, не только один файл — подтверждено: ошибки печатаются
  и для `vec_f32_chained_debug.nv`, другого peer-файла) — RED, `[E7301]`,
  ИДЕНТИЧНО ошибке из 208-f4r-notes.md Ш2. Также `nova-codegen test-build`
  на той же папке (whole-folder CU) — тот же CODEGEN-FAIL с тем же E7301
  для d374 И vec_f32_chained_debug.nv (МНОЖЕСТВЕННЫЕ файлы страдают —
  подтверждает, что коллизия CU-глобальная, не d374-специфичная).
- `nova check` БЕЗ цикла (baseline, 16-файловый protocol-fixture subset:
  d42/d53/d72/d355/d374/protocol_lit×11) — PASS 16/0, WARN:1152 (все —
  unused-import/W_FFI_CANCEL_UNSAFE, предсуществующие, не мои).

## Инструментация (в процессе)

Добавлен temp-debug eprintln в `types/mod.rs` `TypeCheckCtx::build`
(Item::Type ветка, ~3636) — печатает `name/file_id/span_start/n_methods/
already_present` когда `td.name == $NOVA_DBG_TYPE_NAME`. Гейт: env-var,
УДАЛИТЬ перед финальным коммитом. Цель — эмпирически подтвердить: сколько
раз "Write" встречается в `module.items`, из какого file_id, в каком
порядке — до и после цикла. nova-cli пересобирается (b1n25fwgm).

## Инструментация — данные (КОРЕНЬ ПОДТВЕРЖДЁН ОКОНЧАТЕЛЬНО)

`NOVA_DBG_TYPE_NAME=Write` trace (types/mod.rs, `TypeCheckCtx::build`,
Item::Type insert):

**БЕЗ цикла** (baseline):
```
[DBG-TYPE] name=Write file_id=1075 span_start=2667  n_methods=2 already_present=false   ← std.io.Write (flush!) — INSERTED FIRST
[DBG-TYPE] name=Write file_id=1106 span_start=12189 n_methods=1 already_present=true    ← prelude.protocols.Write — INSERTED SECOND, WINS → correct
```
→ PASS (протокол-Write побеждает).

**С циклом** (repro: временный `import std.prelude.protocols.{Fmt}` в
`fmt_buf/core.nv`, снят после подтверждения):
```
[DBG-TYPE] name=Write file_id=1025 span_start=12189 n_methods=1 already_present=false   ← prelude.protocols.Write — INSERTED FIRST (порядок ФЛИПНУЛСЯ)
[DBG-TYPE] name=Write file_id=1076 span_start=2667  n_methods=2 already_present=true    ← std.io.Write — INSERTED SECOND, WINS → E7301
```
→ FAIL (io.Write побеждает — требует `flush()`, StringBuilder не satisfies).

**Вывод**: `std.io.Write` РЕАЛЬНО присутствует в CU в ОБОИХ сценариях (не
цикл его туда приносит) — цикл лишь ФЛИПАЕТ относительный порядок вставки
двух РАЗНЫХ протоколов с одинаковым именем в ОДНОЙ global-by-bare-name
HashMap (`types: HashMap<String, &TypeDecl>`, `types/mod.rs`
`TypeCheckCtx::build` ~3636). НЕ дублирование одного протокола через два
пути цикла (гипотеза брифа была неточной) — коллизия ДВУХ РАЗНЫХ типов.

## ФИКС РЕАЛИЗОВАН

`compiler-codegen/src/types/mod.rs`:

1. `file_local_types: HashMap<FileId, HashMap<String, &TypeDecl>>` (поле
   `TypeCheckCtx`, existing side-table — до этой волны заполнялось ТОЛЬКО
   для `td.file_private` типов, [M-198-f4c-1-privfile-type-not-discriminated]).
   **Обобщено**: теперь заполняется для ЛЮБОГО `Item::Type` (убран gate
   `if td.file_private`), ~3636-3663 (`build`). Побочных коллизий не
   создаёт — один файл не может дважды объявить один и тот же bare-имя.
2. `protocol_mismatch_found` (~14526-14550): было
   `let td = self.types.get(name)?;` (CU-глобальный last-write-wins слот).
   Стало `let td = self.types_get_for_file(name, span.file_id)?;` —
   `span` теперь захватывается из `TypeRef::Named { path, span, .. }`
   (был `..`, отброшен). `types_get_for_file` — УЖЕ существующий метод
   (~4096), сначала смотрит per-file overlay для `span.file_id` (файла,
   где ФИЗИЧЕСКИ написан референс `sink Write`, т.е. protocols.nv), падает
   назад на глобальный `self.types` только если в этом файле такого имени
   нет. Идемпотентно/детерминированно по declaring-file — НЕ зависит от
   порядка мержа импортов, НЕ спец-ветка под fmt_buf/Write (общий
   механизм, работает для ЛЮБОЙ cross-module same-name protocol-коллизии).

НЕ тронуто: imports.rs (только читал), emit_c.rs, closure-peek (~16769),
check_consume.

## Итерация 2 фикса (первая была неполной — embed-рекурсия)

Первая версия фикса (только `protocol_mismatch_found`'s прямой
`types_get_for_file`) убрала E7301 для `d374`'s ПЕРВОЙ строки
(`FmtCtx.bare(sb,...)`), но всплыла НОВАЯ: `p.display(fmt_ctx)` →
`[E_NO_MATCHING_OVERLOAD]`. Корень: `protocol_missing_methods` (метод-
список для "Fmt" при проверке `FmtCtx` ⊨ `Fmt`) делал СВОЙ независимый
`self.types.get(proto_name)` (СТАРЫЙ путь, не переиспользующий `td` уже
резолвленный в `protocol_mismatch_found`) — плюс, при рекурсии в `use`-
embeds (`Fmt` embeds `Write`), file_id, который нужно использовать для
резолва ИМЕНИ EMBED'а — это file_id ПРОТОКОЛА-ВЛАДЕЛЬЦА embed'а (`Fmt`
объявлен в protocols.nv → embed `use Write` тоже физически в protocols.nv),
а НЕ file_id исходного CALL-SITE (d374_write_sink_decouple.nv, который САМ
ни `Fmt`, ни `Write` не объявляет → fallback на глобальный слот →
СТАРАЯ коллизия просачивается обратно на embed-уровне).

Фикс: `protocol_missing_methods` получил параметр `use_file_id`
(threaded из `Req::Named(name, file_id)` — enum `Req` расширен вторым
полем). Для ВЕРХНЕГО вызова — file_id референс-сайта (как раньше).
Для РЕКУРСИВНЫХ embed-вызовов — используется `td.span.file_id` (file_id
ТЕКУЩЕГО протокола, чьи embeds обрабатываются), а НЕ проброшенный сверху
use_file_id — принцип: имя embed'а резолвится относительно файла, где
ФИЗИЧЕСКИ написан `use X` (тот же файл, что объявляет embed'ящий
протокол), не относительно файла исходного call-site.

После итерации 2: `nova check d374_write_sink_decouple.nv` (с циклом,
whole-folder-CU) → **PASS 1/0** (было FAIL и на E7301, и на
E_NO_MATCHING_OVERLOAD после первой итерации).

## Финал — все гейты зелёные

- `nova check` d374 (whole folder-CU via peer merge) — репро-цикл RED→GREEN
  подтверждено дважды (стабильно, не флак).
- Без цикла (после revert) — тот же 20-файловый protocol-fixture набор:
  **PASS 16 / FAIL 4** (identично результату С циклом — neg_protocol_param_*
  4 файла корректно FAIL, это их роль как EXPECT_COMPILE_ERROR фикстур, не
  регрессия).
- `nova test std/src/checksums` — **PASS 3/0**.
- `nova test std/src/collections` — **PASS 13/0**.
- Флагман `examples/flagship/aggregator/src/main.nv --strict-effects` —
  **built чисто** (54.77s, только предсущ. unused-import/W_DEP_PATH warnings).
- Мега-CU (весь `spec_tests/conformance` целиком, ~169 файлов) — НЕ гонялся:
  заблокирован НЕСВЯЗАННЫМ pre-existing ICE (`[M-write-at-p67-legacy-ice-
  conformance-folder]`, новый маркер в backlog-followups.md, P1, чужая зона
  closure-peek/check_consume) — задокументировано отдельно для
  интегратора/владельца, НЕ чинилось (вне брифа этой волны).
- Репро-цикл (temp `import std.prelude.protocols.{Fmt}` в `fmt_buf/core.nv`)
  — снят, файл вернулся к оригиналу (`git status` подтверждает 0 diff).
- Debug-инструментация (eprintln в `types.insert`) — снята полностью, не
  попала в финальный diff.
- Маркер `[M-fmt-write-protocol-collision-cycle-adjacent]` закрыт в
  `docs/plans/backlog-followups.md` (✅ DONE).
- Язык НЕ меняется (идемпотентность/дедуп регистрации, чисто компиляторный
  fix типо-резолва) — D-амендмент НЕ нужен.

## Статус: ЗАВЕРШЕНО — коммит в ветку `p-fix-write-collision`, в main НЕ мёржено, push НЕ делался
