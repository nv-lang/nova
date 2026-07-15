<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 209 Ф.1 — checkpoint notes (sonnet, worktree nova-209f1 / branch plan209-f1)

База: `ebd11ca4e` (main, включает 209-recon-notes.md). Работа ведётся ТОЛЬКО в
`d:/Sources/nv-lang/nova-209f1`; суб-агенты не спавнились.

## Дизайн-решение (уточнение поверх рекона, принято в ходе исполнения A1)

Рекон §5 предлагал два кандидата (dual-buffer vs post-finalize splitter) и
рекомендовал (B) post-finalize. При реализации A1/A2 дополнительно решено:

- **A1 трогает ТОЛЬКО эмиссию ОПРЕДЕЛЕНИЙ** (тело `{ ... }` или global-с-
  инициализатором) и их парных forward-decl (где они существуют рядом).
  НЕ требуется гоняться за каждым разрозненным fwd-decl call-site вручную —
  A2 (`split_tu`) сам классифицирует top-level unit'ы и **дедуплицирует**:
  если decl-only сегмент (кончается `;`, без тела) назван так же, как
  ОПРЕДЕЛЕНИЕ где-то ещё в выводе — decl отбрасывается, а common.h получает
  **авторитетный прототип, сгенерированный из самого определения** (сигнатура
  до `{`). Это резко снижает риск «пропущенного сайта»: даже если какой-то
  старый fwd-decl остался `static` (недосмотр), это либо (a) безвредно
  отброшено дедупом, либо (b) если сам ОПРЕДЕЛЕНИЕ забыто — это будет ГРОМКОЙ
  ошибкой компиляции С (conflicting static/non-static) внутри одного part'а,
  а не тихой порчей — что и есть желаемый safety-net (перекрывает A3).
- Два хелпера: `top_level_storage()` (`"static "` / `""`) для НЕ-inline
  top-level определений/глобалов, и `top_level_storage_inline()`
  (`"static inline "` / `""`) для сайтов, которые СЕЙЧАС пишутся как
  `static inline`, но НЕ входят в список «оставить inline навсегда»
  (nova_opt_eq_*, once/lazy property-методы, NovaRes_ конструкторы,
  Nova_<T>_zero_storage, Nova_<Effect>_<method> dispatch-обёртки, VR/struct eq
  wrapper'ы) — recon §4 явно относит их тела к «ровно ОДИН part», значит при
  multi-TU `inline` тоже снимается (mixing extern+inline поперёк TU — свой
  источник проблем; plain external проще и корректно).
- **Ровно 2 постоянных исключения** (всегда `"static inline "`, ЛЮБОЙ флаг):
  `nova_typeid_user_name` (`emit_c.rs` тело в `tid_defines`, доka-marker
  `__TYPEID_DEFINES__`) и per-E throw fast-path `_nova_throw_typed_{m}`
  (`register_fail_e_type`/`render_per_e_fail_decls`). Обе целиком попадают в
  `common.h`-подобную зону и годятся для per-TU дублирования по дизайну.
- **Уточнение к recon §2**: per-E Fail TLS-слот (`_nova_handler_Fail_{m}`,
  оба ветвления `_MSC_VER`/`__thread`) — рекон уже пометил его как
  «обязан быть одно определение + extern» — это НЕ inline-исключение (только
  throw-функция инлайнится); слот промоутится через `top_level_storage()`.
- **Уточнение к recon §4** (типо-инфо): `NOVA_TYPEINFO_<sani>` (`static const
  NovaTypeInfo`) — рекон пометил «безопасны как per-TU дубль, но кладём по
  единому инварианту (единое определение)» → промоутится через
  `top_level_storage()`, НЕ оставлен как дубль-per-TU.
- Локальный `static int _nova_decreases_depth_<fn>` (block-scope internal
  counter внутри `emit_fn`, decreases-contract guard) — НЕ трогается: это
  C block-scope static, area видимости — только внутри функции, cross-TU не
  наблюдаем в принципе (оставлен hardcoded `"static "`).
- `eq_needle`-поиск в hoist-логике NovaOpt typedef (~line 15049,
  `register_novaopt_decl`-соседний код) — needle тоже переведён на
  `top_level_storage_inline()`, иначе под флагом needle перестал бы матчиться
  и hoist молча стал бы no-op (нашёл при вычитке, не в исходном рекон-списке).

## A1 — статус: ГОТОВО

- Хелперы `top_level_storage()` / `top_level_storage_inline()` + поле
  `multi_tu_enabled: bool` (env `NOVA_MULTI_TU=1`/`true`), добавлены в
  `CEmitter` (emit_c.rs, рядом с `record_field_names`/`CEmitter::new()`).
  Дефолт `false` → байт-идентичный путь (каждый сайт при флаге off печатает
  ровно то же `"static "`/`"static inline "`, что и до Plan 209).
- Проведено через хелперы: **~60 сайтов** эмиссии (после дедупа парных
  decl+def как один «сайт» в отчёте) — free/method fn (regular + `__sret`),
  mono fn/method (+ `__sret`), lambda/trailing-block/spawn/drain/detach/
  blocking entry-функции, effect-handler impl (все 3 вариации), test/bench/
  test-chunk функции, `nova_consts_init`, `nova_fn_main_impl` (все 3 сайта),
  `_nova_register_all_effects_`, sum-variant/mono-variant `nova_make_*`
  конструкторы, associated consts, lazy-const storage + interned
  string/blob consts, per-E Fail TLS-слот (оба ветвления), NovaTypeInfo
  consts, VR/struct-eq wrapper'ы (binop/unop/named), `nova_opt_eq_*` (все 8
  вариантов + needle), NovaRes_ конструкторы (3), once-cell/Lazy generic-type
  property-методы (9 функций), closure singleton globals + thunk'и.
- Оставлено БЕЗ изменений (намеренно): `nova_typeid_user_name` (7434),
  per-E throw fast-path (2296), block-scope `static int` decreases-counter
  (24098).
- Верификация: `cargo build --release -p nova-codegen` — **0 ошибок**
  (только пред-существующие `never used` warnings). Полный ребилд после
  `touch emit_c.rs` — 37s, чисто.
- Байт-идентичность дефолта: НЕ инструментальный diff (то дело A2/verify),
  но по конструкции каждый переведённый сайт при `multi_tu_enabled == false`
  печатает БУКВАЛЬНО ту же строку (`"static "`/`"static inline "` — те же
  literal-байты, просто через `{}`-плейсхолдер вместо hardcoded текста).
  Формальная проверка diff'ом — см. раздел Verify ниже (сделана после A2/A4).

## A2 — статус: ГОТОВО (`compiler-codegen/src/codegen/split_tu.rs`, новый модуль)

`pub fn split_tu(finalized: &str, cu_name: &str, threshold_bytes: usize) -> Result<SplitResult, String>`.
Чистая текстовая трансформация (без AST) поверх уже-финализированной строки.

Алгоритм (детали — doc-комментарий в начале файла):
1. `segment_top_level` — скан на глубине `{}` (строки/char-литералы/`//`/`/* */`
   комментарии не влияют на depth), детерминированно режет на contiguous
   top-level units (конкатенация всех units == вход, проверено тестом).
   Спецкейсы: preprocessor-директива (`#include`/`#define`/…, с backslash-
   continuation) — своя единица; АТОМАРНЫЙ `#if*/#endif`-блок (с учётом
   вложенности — внутренний `#endif` не закрывает внешний) — целиком одна
   единица (иначе `#ifdef X … #else … #endif`, разъехавшийся по разным
   файлам, разбалансировал бы препроцессор).
2. `classify_unit` — `#include`/`#define`/typedef/decl-only → common.h;
   тело функции / global-с-инициализатором → part (round-robin по
   `threshold_bytes`, одна единица никогда не режется поперёк parts);
   `NOVA_BENCH_STATE_DEFINE`/`NOVA_BENCH_HEAP_SAMPLER_THREAD_DEFINE`
   (табличные известные макро-statement'ы, реально разворачивающиеся в
   global storage — bench.h) — всегда part-bound.
3. Атомарный cond-блок, содержащий внутри определение (найдено рекурсивным
   `cond_block_contains_definition`/`split_cond_block_pieces`, различающим
   директивы ЭТОГО уровня от вложенных) — блок ЦЕЛИКОМ в один part; в
   common.h — зеркало (`mirror_cond_block_as_decl`): те же директивы,
   тела/инициализаторы заменены на прототип/`extern`. Покрывает реальный
   кейс per-E Fail TLS (`#ifdef _MSC_VER … #else … #endif` вокруг
   `_nova_handler_Fail_<m>`).
4. Дедуп: decl-only unit, чьё имя совпадает с найденным где-либо
   определением, — отбрасывается (заменяется auto-generated прототипом из
   самого определения). Значит A1 не обязан был поймать КАЖДЫЙ
   разрозненный fwd-decl call-site вручную — пропущенный сайт либо
   безвредно дедуплицируется, либо (если пропущено само определение)
   аукнется ГРОМКОЙ ошибкой компиляции C (conflicting static/non-static)
   внутри одного part'а — не тихой порчей.
5. Сборка: common.h = effect-count строка 1 (verbatim, всегда) + include-
   guard + все header-единицы (исходный порядок) + auto прототипы/externs.
   `_partK.c` = `#include "<cu>_common.h"` + назначенные определения.

Unit-тесты (18, все зелёные — прогнаны standalone `rustc --test`, см. ниже
почему не через `cargo test`): сегментация воспроизводит вход байт-в-байт;
строки/комментарии с `{`/`}` не сбивают скан; fn-def → имя+прототип;
global-def (скаляр И brace-инициализатор) → имя+extern; typedef с телом →
header verbatim; известный bench-макро → part, не header; cond-блок БЕЗ
определения → header verbatim; cond-блок С определением → атомарно в один
part + zеркало-декларация в header; вложенный `#ifdef` не закрывает внешний
преждевременно; дефолтная форма (common.h + 1 part); дедуп затирает
устаревший fwd-decl; round-robin по байт-порогу (>1 part) с гарантией "одно
определение — ровно в одном part"; определение никогда не режется поперёк
parts даже при абсурдно малом пороге; A3 duplicate-name кейс (см. ниже).

## A3 — статус: ГОТОВО (встроено в `split_tu`, не отдельная функция)

Инвариант уникальности: во время сбора `defined_names` (`HashSet`) любой
повтор имени `FnDef`/`GlobalDef` → `split_tu` возвращает `Err(...)` с
именем символа-нарушителя (вместо тихого молчания/паники) — это ГЛАВНЫЙ
риск плана (promotion `static`→external на не-уникальном имени). Плюс
`debug_assert` (вторая половина A3): каждое определение имеет свою
декларацию/прототип фактически в тексте `common_h` (тривиально по
построению — auto-generated проверка добавлена как страховка на будущее
рефакторинг). Покрыто тестом `split_tu_a3_rejects_duplicate_top_level_definition_names`.

## Инфраструктурная находка (вне периметра, НЕ исправлено)

`compiler-codegen/src/test_runner.rs:5955` — pre-existing (до Plan 209, не
мой diff) сломанный вызов `codegen_to_c(&nv_path, &src, None, false)` —
сигнатура давно требует `ast::ContractsMode`, не `bool`; ломает ВЕСЬ
`cargo test --lib` для crate (compile error), никак не связано с Ф.1.
Не трогал (не в периметре задания; другой автор мог тоже работать в этом
файле). Из-за этого верификация `split_tu` юнит-тестов сделана окольным
путём: тот же файл скомпилирован `rustc --edition 2021 --test` standalone
(модуль не имеет внешних зависимостей кроме `std::collections::HashSet`) —
все 18 тестов зелёные. `cargo build --release` (сам crate, не test) —
чисто, 0 ошибок, split_tu участвует в обычной сборке.

## A4 — статус: ГОТОВО

`CEmitter::emit_module_multi_tu(self, module, cu_name) -> Result<(EmitOutput, Vec<String>), String>`
(emit_c.rs, сразу после `emit_module`). **`emit_module` не тронут вообще** —
back-compat буквально: все существующие вызывающие (main.rs/test_runner.rs/
bench/run.rs) продолжают звать `emit_module` напрямую и получают ТОЧНО ту
же форму, что и всегда. `emit_module_multi_tu` — новая, НИКЕМ ПОКА НЕ
ВЫЗЫВАЕМАЯ обёртка (её будущий вызывающий — Ф.2 тулчейн, вне периметра
Ф.1): гоняет ту же эмиссию, затем ЕСЛИ `NOVA_MULTI_TU` включён И
`exceeds_multi_tu_threshold` (>2МБ ИЛИ примерно >200 `") {"`-вхождений,
дешёвая линейная эвристика — точный подсчёт функций дублировал бы работу
`split_tu` на большом CU, что и так дорого) — вызывает `split_tu` и
возвращает `EmitOutput::Split`; иначе — `EmitOutput::Single` (тот же
`String`, что дал бы `emit_module`). `pub enum EmitOutput { Single(String),
Split { common_h: String, parts: Vec<String> } }` экспортирован из
`codegen::mod`.

Байт-идентичность дефолта — ПОДТВЕРЖДЕНА ЭМПИРИЧЕСКИ (не только по
построению): собран `nova-cli` (release) в этом worktree И отдельно в
temp-worktree на базовом коммите `ebd11ca4e` (`d:/Sources/nv-lang/nova-209-baseline-tmp`,
удалён после проверки), оба прогнаны на `examples/getting_started.nv`
(`NOVA_MULTI_TU` не установлен, `NOVA_CACHE=0`), `.c` вытащены через
`--keep-artifacts`. `diff` показал ТОЛЬКО reorder 5 строк
`typedef struct Nova_X Nova_X;` (порядок HashMap-итерации — **pre-existing
недетерминизм, НЕ вызван Plan 209**: воспроизведён между ДВУМЯ прогонами
ОДНОГО И ТОГО ЖЕ nova-209f1-бинаря с NOVA_MULTI_TU не установлен — тот же
разъезд). После нормализации (те же 5 строк как SET, не порядок) — ОСТАЛЬНОЙ
файл (3970 из 3974 строк) побайтово идентичен baseline. Вердикт: байт-
идентичность дефолтного пути подтверждена (да), с уже известной
неродственной оговоркой про typedef-порядок.

## ⚠ КРИТИЧЕСКАЯ НАХОДКА (вне периметра A1-A4, но обязана быть в отчёте)

При сквозной проверке (`NOVA_MULTI_TU=1`, тот же `getting_started.nv`,
compile+link — **это Ф.2/оркестратор объём, но я прогнал точечно для
самопроверки A1**) — **линк падает**: `undefined symbol:
Nova_HashMap_method_merge_from` / `Nova_HashMap_method_values`,
воспроизводится 3/3 раз (при NOVA_MULTI_TU не установлен — 3/3 успех).

**Корень (найден, НЕ исправлен — вне периметра Ф.1):** `Set[T]` embeds
`HashMap[...]`; delegated-method wrapper'ы (emit_c.rs, блок
`target_field`/`base_c_name`, ~emit_type_decl, метод типа "embed
delegation") **безусловно эмитятся для типа-обёртки** (`Nova_Set_method_
merge_from`, `Nova_Set_method_values`), вызывая base-метод `HashMap` по
ИМЕНИ (`base_c_name` из `method_overloads`) — но НИКОГДА явно не
регистрируют/enqueue'ят monomorphization этого base-метода (или самого
generic-type-instance `HashMap[...]`) в `mono_worklist`/generic-type
worklist. Если пользовательская Nova-программа не вызывает `HashMap`-метод
НАПРЯМУЮ нигде больше — тело `Nova_HashMap_method_merge_from`/`_values`
НИКОГДА не эмитится. **Раньше это маскировалось `static`+dead-code-
elimination**: `Nova_Set_method_merge_from` тоже static и (если
пользовательский код не зовёт `.merge_from()` на Set) недостижим →
компилятор (`-O2`) целиком выкидывает его ДО того как линкер увидел бы
ссылку на несуществующий `Nova_HashMap_method_merge_from`. Промоушн в
external (A1, `top_level_storage()`) убирает эту DCE-защиту: компилятор
обязан считать функцию потенциально вызываемой из другой TU → тело
остаётся → линк падает на реально отсутствующем symbol'е.

**Вывод.** Это pre-existing дефект mono-регистрации embedded/delegated
методов (маскировался static-DCE), НЕ вызван и НЕ специфичен для
`split_tu`/multi-TU — voзникнет от ЛЮБОГО механизма, снимающего `static` с
подобных wrapper'ов. Обнаружен ИМЕННО благодаря Plan 209 (промоушн
делает его наблюдаемым). **НЕ исправлял** — вне периметра Ф.1 (это
отдельная инвестигация в mono-collector для embedded-полей generic-типов,
не "codegen split"); попытка блиц-фикса вслепую рискованна (могло быть
глубже: сам generic-type-instance `HashMap[...]` мог не enqueue'иться, не
только его метод).

**Рекомендация для Ф.2/Ф.3 (владельцу):** ПЕРЕД широким включением
`NOVA_MULTI_TU` для реальных CU — либо (а) исправить mono-регистрацию
embedded/delegated методов (usage-discovery pre-pass должен видеть сквозь
embed-поля другого generic-типа), либо (б) принять, что
compile+link-гейт Ф.2/Ф.3 ("линк без multiple-definition") ловит подобные
кейсы как раз по назначению — но нужно ЗАЛОЖИТЬ ВРЕМЯ на их разбор
(вероятно НЕ единичный кейс — паттерн "delegated-method wrapper без явного
enqueue base-метода" может повторяться где угодно, где generic-тип
embed'ится в другой generic-тип). Split_tu/A1-A4 сама механика — корректна;
это независимый codegen-дефект, который multi-TU (или ЛЮБОЙ non-static
рефакторинг) неизбежно вскрывает.

## Итог Ф.1 (сжато)
- A1: ГОТОВО. ~60 top-level static-сайтов через `top_level_storage()`/
  `top_level_storage_inline()`. cargo build: 0 ошибок.
- A2: ГОТОВО. `split_tu` — сегментатор+классификатор+auto-decl+дедуп.
  18 unit-тестов зелёных (standalone rustc, см. выше почему не cargo test).
- A3: ГОТОВО. Инвариант уникальности встроен в `split_tu` (`Result<_,
  String>` на дубликате), + debug_assert на common.h-покрытие.
- A4: ГОТОВО. `emit_module_multi_tu` back-compat обёртка + порог-гейт;
  `emit_module` не тронут. Байт-идентичность дефолта подтверждена эмпирически.
- ⚠ Критическая находка (вне периметра): mono-регистрация embedded/
  delegated методов имеет pre-existing дыру, маскировавшуюся static-DCE;
  промоушн (A1) её вскрывает. Задокументировано выше, требует отдельной
  волны ДО широкого Ф.2/Ф.3 rollout.
- Полный `nova test`/conformance НЕ гонялся (вне периметра, дорого/долго —
  явно исключено заданием). Мега-CU split ON — НЕ прогонялся руками на
  реальном 13Мб CU (тоже вне периметра Ф.1 по заданию); структурная
  корректность split_tu подтверждена unit-тестами на синтетических .c,
  покрывающих все классы конструкций, реально присутствующие в emit_c.rs
  выводе (typedef, decl-only, fn-def, global-def скаляр/brace-init,
  known-macro, cond-block с/без определения, вложенность).
