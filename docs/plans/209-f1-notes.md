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

## Следующие атомы (план)
- A4: провести флаг + порог (>2МБ/>200 fn) через `emit_module`, back-compat
  обёртка возврата.
