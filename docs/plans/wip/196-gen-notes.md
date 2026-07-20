<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Зона GEN: чекпоинт-заметки (sonnet, worktree `nova-196gen`)

**Назначение файла:** непрерывность при обрыве сессии + сырые находки для интегратора (приёмка
ПО КОДУ, интегратором). Файл переиспользуется разными GEN-подзадачами во времени — предыдущее
содержимое (сессия `p196-zone-gen`, широкая кампания Q9/Q10/D239/D372, emit_c 19485-21432) уже
проинтегрировано/устарело относительно текущей ветки; см. `git log -p` этого файла для истории,
если нужно восстановить контекст той сессии.

---

## ТЕКУЩАЯ СЕССИЯ: снос легаси-движков резолва Result/Option (по карте CH)

**Ветка:** `p196-gen-result-teardown` (от `main` `bbfd63e3b`). **Карта:** CH-агент,
`docs/plans/wip/196-ch-result-notes.md` §6 «Разблокировка для GEN-сноса».

### Итог одной строкой

Keystone-задача карты — перенос `register_novares_decl`/`register_novaopt_decl` side-effect'а из
легаси `resolve_result_option_ret` (~emit_c.rs:19211) в «законный emit-pass sink». По коду
выяснилось: такой sink уже существовал для Result (`result_repr_c_type`, ~48486, свой же
196.3-вердикт «LEGIT-LOWERING, canonical sink»), но НЕ для Option (инлайн-дубль внутри
`resolved_named_to_c`'s `"Option"`-ветки, ~4127). Сделано: добавлен Option-твин
`opt_repr_c_type` (~48492) + ОБЕ стороны (channel-producer `resolved_named_to_c` И legacy
`resolve_result_option_ret`) теперь зовут ОДИН канонический sink на каждый тип (Result→
`result_repr_c_type`, Option→`opt_repr_c_type`) вместо независимого byte-identical дублирования
логики. Это и есть факт «переноса side-effect в emit-pass»: typedef-регистрация больше НЕ
дублируется в type-inference хелпере.

**Полный снос (удаление тела/безусловный panic) `resolve_result_option_ret` НЕ произведён** — по
структурной причине кода (не по недосмотру), см. «Почему снос не завершён» ниже.

### Что сделано (файл:функция, маркер `[M-196-gen]`)

`compiler-codegen/src/codegen/emit_c.rs`:

1. Новый sink `opt_repr_c_type(&self, inner_c: &str) -> String` (~48492, рядом с
   `result_repr_c_type`) — Option-твин: `sanitize_for_novaopt` + `register_novaopt_decl` +
   `format!("NovaOpt_{}", ..)`.
2. `resolved_named_to_c`'s `"Option"`-ветка (~4127) — инлайн заменён на `self.opt_repr_c_type(&inner_c)`.
3. `resolve_result_option_ret` (~19211):
   - Result-ветка → `self.result_repr_c_type(&ok, &err)` (было: ручные `register_novares_decl` + `format!`).
   - Option-ветка → `self.opt_repr_c_type(&inner)` (было: ручные `register_novaopt_decl` + `format!`).
   - Добавлены debug-only (`cfg(debug_assertions)`, `NOVA_TRACE_ICR=1`) `icr_trace` маркеры
     `GEN196_legacy_resolve_result_option_ret_RESULT`/`_OPTION` — та же нулевая-overhead
     конвенция, что и остальные ~114 ICR-бакетов в `infer_call_ret_c`; даёт будущей волне точку
     измерения достижимости ИМЕННО этой функции (не косвенно через B06a/B10j, которые триггерятся
     любым из 3 fallback'ов каскада).

Pure refactor — идентичные аргументы/порядок вызовов/строки форматирования. Ни одно вычисляемое
значение не изменилось ни в одной ветке.

### Почему снос не завершён (структурная причина, не недосмотр)

Оба вызывающих сайта `resolve_result_option_ret` — ВНУТРИ frozen wave-1 `infer_call_ret_c`
(фактический диапазон сейчас `50757`-`52899`; задание давало устаревшие координаты `46293`-`48883`
из 196.5-заморозки — файл вырос на ~4400 строк, функция та же, просто съехала):

- `~51162` (`B06a_method_overload_sentinel_mono`) — METHOD-level-generic класс
  (`mono_method_decls` sentinel). Карта CH отмечает этот класс ВНЕ ПЕРИМЕТРА (Producer B,
  `resolve_return_channel` method-level widen — «вне этой волны»).
- `~51879` (`B10j_generic_fn_value_aware_return`) — free-fn/static-ctor generic возврат. ICR-трейс
  НЕ бьёт ни на одной проверенной фикстуре (согласуется с находкой CH — канал уже отвечает раньше
  в каскаде).

`resolve_result_option_ret` не имеет expr-id/call-site identity в сигнатуре — не может различить,
чей вызов до неё дошёл. Единственный способ детачнуть ТОЛЬКО free-fn/ctor-класс без риска для
method-класса — редактировать сами вызывающие сайты внутри `infer_call_ret_c` (различитель), что
прямо запрещено заданием («НЕ ломать frozen-контракт... перенос side-effect, НЕ переписывание
frozen-логики»). Безусловный panic внутри функции стрелял бы по ОБОИМ сайтам одинаково — для
B06a (method-класс, доказанно не покрытого продюсером в эту волну) это была бы недоказанная
регрессия, не снос.

Это ровно условие из `docs/plans/196.4-call-resolvedtype-channel.md` §9: «удаляются... ИЛИ раньше
через `panic!`-detach, ЕСЛИ ветвь отделима без правки замороженного диапазона» — не отделима в эту
волну → остаётся 🔄 (§9/§10 того же документа явно это разрешают). Функция сохранена ЖИВОЙ во всех
профилях (включая release) — корректный fallback для METHOD-класса.

### Empirical trace (доказательство «дальше сносить рано»)

Debug build, `NOVA_TRACE_ICR=1`, `nova-codegen compile <fixture>` standalone:
- Гейт-фикстуры: `d85_question_return`/`d85_result_payload_width` — 0 хитов; `d30_try_op_unwrap_pair`
  — `B10f`/`B11d`/`B11r` (METHOD non-generic, ожидаемо); `d408_option_chain_sized_width` — `B11q`
  (METHOD, ожидаемо). Ни разу `B06a`/`B10j`/`GEN196_legacy_resolve_result_option_ret_*`.
- CH-пробники: `d30_result_option_ret_generic` — `B10e`; `d88_default_generic_params` — 0;
  `m196_facetc_generic_static_typaram` — `B12o`. Тот же результат — 0 `B06a`/`B10j`/`GEN196_*`.
- Корпус `std/src/{collections,time,encoding}` (97 файлов, standalone): 12/97 скомпилировались
  (остальные 85 требуют full-CU/folder-module контекст — `nova-codegen compile` берёт ОДИН файл,
  не резолвит co-equal siblings; подтверждено на `d325_result_everywhere.nv`, module
  `spec_tests.conformance`, не резолвит `sequence`/`partition` из `std/src/prelude/core.nv` в
  standalone-режиме — implicit prelude подключается только в manifest-aware `nova build`/`nova
  test`, НЕ в raw `nova-codegen compile <file>`). Из 85 несобравшихся — 12 упали
  ПРЕДСУЩЕСТВУЮЩЕЙ Rust-панико́й `[P67-LEGACY] ... return type unknown` (52787/52930/53787) — НЕ
  про Result/Option (`.append`/`.swap`/`.keys`/`.reserve`/`.iter`/`.len`/Path-`new`/`max_value`),
  триггерится нехваткой multi-file контекста, не этой правкой. 12 успешных компиляций — 0
  `B06a`/`B10j`/`GEN196_*`.

Вывод: на всём достижимом standalone-материале (7 узких фикстур + 12 корпусных файлов)
`resolve_result_option_ret` ни разу не сработала ни для method-, ни для free-fn-класса —
согласуется с картой CH. НЕ исчерпывающее покрытие (flagship/nova_tests/85 недостижимых std-файлов
вне охвата этого инструмента) — недостаточно для безусловного panic, достаточно чтобы держать
debug-only trace живым для накопления доказательства (тот же паттерн, что уже применён в этом
файле к `B10l`/`B10m`: «Детач+panic ПРОБОВАЛСЯ — 0 fires... NOT removed»).

### Финальные гейты (release nova-cli, после исправления libuv-copy bug)

Первая попытка `nova test spec_tests/conformance` упала `FATAL libuv submodule not initialized` —
причина: мой РАННИЙ `cp -r` в целевую папку, которая уже существовала (пустая), дал вложенный
`libuv/libuv` (та самая ловушка из `project-worktree-nova-test-setup` — «если целевая папка уже
есть — сначала `rm -rf`»). Исправлено (`rm -rf` + повторный `cp -r` + удаление вложенного `.git`).

- **conformance ОДНИМ compile-unit'ом** (`nova test spec_tests/conformance --jobs 4`, release
  nova-cli): **PASS: 124  FAIL: 0  SKIP: 14**. Главный гейт (CLAUDE.md) — ЗЕЛЁНЫЙ. `d325_
  result_everywhere` компилируется и проходит ВНУТРИ этого прогона (полный manifest-aware CU
  резолвит `sequence`/`partition`/`to_int`, которые raw `nova-codegen compile <file>` не резолвит
  — см. §byte-parity выше; ограничение было именно инструмента `compile`, не языка/этой правки).
- **Флагман** `nova check --strict-effects examples/flagship/aggregator/src/main.nv`: **PASS: 1
  FAIL: 0 WARN: 28** (все warning — unused-import/postfix-mut-канон, косметика, не про эту правку).
  Улучшение относительно CH-чекпойнта (§0 карты): та сессия видела pre-existing `nova-tls`
  `E_CONSUME_PATTERN_REQUIRED` FAIL — не воспроизвелось здесь (git-dep кэш/апстрим успел
  обновиться между сессиями; не мой предмет).
- **Флагман full build** `nova build --strict-effects --mode release examples/flagship/aggregator/
  src/main.nv -o aggregator.exe`: **built: aggregator.exe (49.65s)**, 0 ошибок. Полный C-codegen +
  компиляция + линковка прошли чисто.

Все три гейта зелёные на release-бинаре (нет debug_assertions → `icr_trace`/`GEN196_*` markers
скомпилированы в no-op, нулевой overhead, поведение идентично pre-refactor коду по построению).

### Byte-parity (диф .c ДО/ПОСЛЕ, standalone, debug build)

| Фикстура | diff |
|---|---|
| `d85_question_return` | 0 |
| `d85_result_payload_width` | 0 |
| `d30_try_op_unwrap_pair` | 0 |
| `d408_option_chain_sized_width` | 0 |
| `d30_result_option_ret_generic` | 0 |
| `d88_default_generic_params` | 0 |
| `m196_facetc_generic_static_typaram` | 0 |

`d325_result_everywhere.nv` — standalone-compile недостижим ИНСТРУМЕНТОМ (см. корпус-абзац выше;
`NOVA_STD_PATH` не помогает — ошибка не про поиск пути, а про implicit-prelude, который raw
single-file `compile` не подключает). НЕ регрессия правки — доказуемо ПО ФАЗАМ: d325's ошибки
(`undefined identifier`/`E_UNKNOWN_METHOD`) — диагностика TYPE-CHECK фазы (`types/mod.rs`), которая
падает ДО того, как codegen (`emit_c.rs`, где вся правка) вообще запускается; d325 физически не
может увидеть эту правку ни в каком виде (идентичный отказ до/после на identical stage). Result/
Option-канал на этом классе косвенно покрыт `d30_try_op_unwrap_pair`/`d30_result_option_ret_generic`
(Result) + `d408_option_chain_sized_width` (Option) — все diff=0. Снятие ограничения для d325 самого
требует `nova build`/`nova test` (manifest-aware pipeline) вместо raw `nova-codegen compile` —
вне этого чекпойнта; полный mega-CU НЕ гонялся (задание: «Мега-CU НЕ гонять»).

### Реестр 196 — что закрыто/осталось

- ЗАКРЫТО: дубль-движок typedef-регистрации Result/Option (раньше независимо инлайнился в ДВУХ
  местах — channel-producer `resolved_named_to_c` и legacy `resolve_result_option_ret`) — теперь
  ОДИН канонический sink на тип. «ONE TRUTH» для этой конкретной под-задачи достигнута.
- 🔄 НЕ ЗАКРЫТО (задокументировано, НЕ регрессия): физический снос `resolve_result_option_ret`/её
  вызовов в `infer_call_ret_c` — гейтится (а) METHOD-generic классом (Producer B, вне периметра
  этой волны) и (б) неотделимостью двух call sites без правки frozen-диапазона (запрещено
  заданием). Debug-only trace-хуки оставлены живыми для будущего накопления доказательства.
- `infer_result_type_params`/`resolve_result_te`/`resolve_result_te_strict` — не трогались: уже
  соответствуют byte-parity-safe migration паттерну (legacy-wins-over-channel guard, ~18081); их
  call sites вне frozen-зоны потенциально доступны будущей волне, но это отдельная, более крупная
  задача (десятки call sites), не keystone этого задания.
- `infer_method_level_return_for_sum`/B11r/B11q — не тронуты (вне периметра, по карте CH).
- `rt_slots_from_args` — не тронут (Producer B / instance-method класс, вне этой волны).

### Гейты (см. финальный отчёт агента для release/conformance/флагман)

Byte-parity 7/7 diff=0; d325 недостижим инструментом (фазовое доказательство неприкосновенности);
детач: debug-only trace добавлен (0 хитов на всём проверенном материале), безусловный panic НЕ
добавлен (недостаточно доказательства по всему корпусу).

Модель: sonnet.
