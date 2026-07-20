<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# [M-linux-mn-conformance-red] — диагностика+фикс, чекпоинт

Worktree: `d:/Sources/nv-lang/nova-linuxrace`, ветка `p-fix-linux-mn-red`,
база `main` @ `58804953d`. WSL-стенд: `~/nova-appeffect-wsl` (rsync-снапшот,
НЕ git repo), `~/nova-appeffect-target` (CARGO_TARGET_DIR). Модель: sonnet.

## Предыстория (из backlog + 211-park-join-research.md §7.5, прочитано ДО старта)

- `app_effect_basic_t8_1` = алфавитно-первый файл ВСЕГО conformance
  folder-модуля (993 co-equal файла, ОДИН CU) — самый большой test-бинарь
  (~117s), статистически чаще ловит редкие M:N-гонки (не что-то в его 21
  строке).
- 2026-07-20 прошлый заход (worktree nova-appeffect, ветка
  p-fix-linux-appeffect): 4/5 RUN-FAIL, `--jobs 1` НЕ убирает (гонка ВНУТРИ
  мега-процесса, не test-runner). Симптом: `assert failed: 27.0.cbrt() ==
  3.0` под label `app_effect_basic_t8_1.nv:22` — но у файла 21 строка и НЕТ
  cbrt(). Похоже на чужой label от `d109_primitive_methods_f64_f32_math.nv`
  (реальная строка 22 там). Корень НЕ локализован (бюджет кончился на этом).
- ПАРАЛЛЕЛЬНО другая волна (rt-headers + gcc15, тот же день) обнаружила и
  ЗАКРЫЛА (см. `docs/plans/wip/gcc15-rt-notes.md`, смёржено в main
  `d3d53f183`) 3 категории gcc15-ошибок компиляции rt-архива (НЕ concurrency
  race — компиляционные) + include-гигиену. Rt-архив теперь ARCHIVE_OK на
  WSL2 gcc15/clang21. Это МОГЛО влиять на предыдущие прогоны (rt-архив
  раньше падал в per-build inline compile fallback на этой машине) —
  сейчас main уже несёт эти фиксы, стенд пересинхронизирован с main HEAD.
- Субстратные data-race фиксы (211 §7.3/§7.4: runq init↔steal 2-phase split,
  preempt_flag/alloc_count atomics) — тоже уже в main, TSan-верифицированы
  0 warnings на синтетике `mn_smoke.c`, но НЕ проверены напрямую на самой
  фикстуре `app_effect_basic_t8_1`.
- known_red список в nova-gate.yml: `app_effect_basic_t8_1` +
  `standalone/supervisor_parfor_test` — остаётся, связь класс→фикстуры не
  доказана индивидуально.

## План этого захода

1. Пересобрать release на WSL (свежий main через rsync) — В ПРОЦЕССЕ.
2. Прогнать представителя ×10 с диагностикой — какой РЕАЛЬНЫЙ тест падает,
   какое РЕАЛЬНОЕ значение (а)/(б)/(в) гипотезы.
3. Фикс по вердикту.
4. ×20 стабильно PASS перед снятием known_red.

## Прогресс

- [x] Worktree `nova-linuxrace` создан @ main 58804953d.
- [x] rsync свежего main → `~/nova-appeffect-wsl` (exclude .git/target/
      nova_rt/libuv[уже populated]/vcpkg_installed/nova_tests).
- [x] release build WSL — чисто, 2m04s (`~/nova-appeffect-target/release/nova`).

## НАХОДКА №1 — (б) label misattribution — КОРЕНЬ НАЙДЕН (deterministic codegen bug, НЕ гонка)

`emit_c.rs::loc_for_span` (строка 2617) вычисляет `(file, line)` для
assert/contract-violation ЧЕРЕЗ `self.source_file_name` (строка-константа,
устанавливается ОДИН РАЗ на всю компиляцию через `set_source_file_name`,
`main.rs:538`/`test_runner.rs:3959` — это путь ENTRY-файла CU, т.е. для
folder-module CU это АЛФАВИТНО-ПЕРВЫЙ файл, `app_effect_basic_t8_1.nv`) и
`self.annotation_source` (тоже ОДНА строка — raw text ТОГО ЖЕ entry-файла,
`set_source_for_annotations`). Но `span.start` (byte-offset), передаваемый
в `loc_for_span` для assert/contract В ЛЮБОМ ИЗ 993 co-equal peer-файлов
folder-module, — это offset ВНУТРИ ЕГО СОБСТВЕННОГО файла (каждый файл
парсится отдельно, offsets НЕ глобальные). `byte_to_line_col` (diag.rs:359)
не имеет bounds-check: если `offset >= source.len()` цикл `for (i,ch) in
source.char_indices()` просто ни разу не находит `i>=offset` и досчитывает
ВСЕ '\n' в WRONG (entry-файл) source до конца → возвращает
`line = 1 + (кол-во '\n' в entry-файле)`.

**Эмпирика:** `app_effect_basic_t8_1.nv` — 21 строка (21 '\n' с учётом
trailing newline) → `byte_to_line_col` для ЛЮБОГО span.start ≥ len(entry
file) детерминированно возвращает `line=22` — ТОЧНОЕ совпадение с
наблюдением прошлой волны («app_effect_basic_t8_1.nv:22», у файла всего 21
строка). Это НЕ гонка, НЕ флака теста — 100% детерминированный compile-time
баг: ЛЮБОЙ assert/contract-fail в ЛЮБОМ non-entry peer-файле folder-module
CU печатается под именем ENTRY-файла с одним и тем же «line=22» (для ЭТОГО
конкретного CU; для другого CU с другим entry-файлом — своя константа =
кол-во строк entry-файла + 1). Компиляторный баг (emit_c.rs), не
test-runner/harness.

**Фикс (не сделан ещё в этой волне — TODO ниже):** `loc_for_span` должен
использовать per-span originating file, не process-global `source_file_name`.
Нужно протащить file-id/file-path per-Expr (или per-Item) через AST для
folder-module merged CU. Зона: `compiler-codegen/src/codegen/emit_c.rs`
(`loc_for_span`, `set_source_file_name`, `set_source_for_annotations`) +
вызовы из `main.rs`/`test_runner.rs`. Это ОТДЕЛЬНЫЙ, самостоятельный баг —
чинить нужно, но НЕ является причиной самого RUN-FAIL (см. находку №2 —
почему assert РЕАЛЬНО падает).

## НАХОДКА №2 — cbrt(27.0)==3.0 деterministически ложно на этом toolchain'е (НЕ гонка)

C-level эксперимент (`/tmp/cbrt_lit.c`, `cbrt(27.0)` без volatile):
- gcc (-O0..-O3): результат **ровно 3.0** (constant-folding, GCC's builtin
  evaluator корректно округляет 27^(1/3)=3 точно).
- clang (-O0..-O3): результат **3.0000000000000004441** (НЕ фолдит,
  реальный RUNTIME-вызов glibc `cbrt()`, который на этой машине даёт 1 ULP
  погрешность для точного куба 27.0 — известный класс погрешности libm
  cbrt на некоторых версиях glibc, НЕ баг Nova).
- С `volatile` (гарантированно runtime call, не constant-fold) — И gcc, И
  clang дают **3.0000000000000004441** на ЛЮБОМ -O — т.е. РЕАЛЬНЫЙ рантайм
  glibc cbrt(27.0) НЕ равен 3.0 на этой машине; только GCC's
  compile-time-constant-evaluator (не настоящий рантайм) даёт точный 3.0.

Nova codegen эмитит **буквальный `cbrt(27.0)`** (`emit_c.rs:47570`,
`f64_method_to_c`) — bare libm-call, без обёртки. `detect_toolchain`
(строка 628-635): **Auto-предпочтение на Linux/macOS = Clang > GCC** — на
ЭТОЙ WSL-машине (оба тулчейна установлены) `nova test` ВСЕГДА выбирает
clang (детерминированно, кэшируется на весь запуск, не альтернирует между
прогонами). Значит: `assert((27.0).cbrt() == 3.0)`
(`d109_primitive_methods_f64_f32_math.nv:24`) на ЭТОЙ машине с clang
компилируется в РЕАЛЬНЫЙ рантайм-вызов libm cbrt(), который возвращает
3.0000000000000004441 ≠ 3.0 — **assert падает ДЕТЕРМИНИРОВАННО, каждый
раз**, не гонка. Это ПЛАТФОРМЕННАЯ (glibc/libm) числовая проблема тестовой
фикстуры, а не M:N-race и не компиляторный баг эмиссии. Нужно решение
владельца/интегратора: (a) фикс фикстуры (ослабить assert на epsilon-сравнение
или через `.round()`), (b) заменить cbrt() реализацию в nova_rt на
скорректированную (напр. `cbrt` через `pow(x, 1.0/3.0)` с post-fixup для
точных кубов — рискованно/лишний скоуп), (c) юз gcc вместо clang на Linux
(не решает — GCC's constant-folding path работает ТОЛЬКО для литералов;
`twentyseven.cbrt()` строка 56 того же файла — cbrt ЧЕРЕЗ ПЕРЕМЕННУЮ, НЕ
литерал → GCC тоже НЕ сможет constant-fold, тоже даст 3.0000...4441 на
runtime call → тоже упадёт даже под gcc). Т.е. (c) не работает — под ЛЮБЫМ
тулчейном на этой libm-версии assert на line 56 (`twentyseven.cbrt()`)
должен падать тоже, т.к. cbrt ИЗ ПЕРЕМЕННОЙ не constant-foldится никогда.
ВЫВОД: фикстура `d109_primitive_methods_f64_f32_math.nv` содержит
платформенно-хрупкий assert (полагается на compile-time constant-folding
конкретного компилятора вместо epsilon-сравнения) — **это НЕ входит в
scope M:N-гонки этой волны**, отдельный маркер нужен.

## Пересечение находок №1+№2 с ЗАДАНИЕМ волны

Обе находки — ДЕТЕРМИНИРОВАННЫЕ баги (компилятор label + фикстура-хрупкость),
НЕ M:N-гонка. Гипотеза задания (б) "attribution" подтверждена частично (label
действительно врёт), но конкретно ЭТОТ файл/тест (d109 cbrt) — не жертва
гонки, а самостоятельный детерминированный дефект. Т.е. "assert failed:
27.0.cbrt()==3.0" — это, возможно, СОВСЕМ НЕ related к M:N-race тема этой
волны; тест ВСЕГДА будет так падать на clang-Linux независимо от
concurrency. Открытый вопрос: закрывает ли исправление №1+№2 ВЕСЬ
[M-linux-mn-conformance-red], или под ними скрывается ЕЩЁ и настоящая
M:N-гонка (сигнатура "падает на выходе"/SIGSEGV из историч. записей). Нужно
изолировать: временно xfail/skip d109's line 24 (или патчить cbrt литерал в
контролируемом эксперименте) и смотреть — падает ли CU ЕЩЁ на чём-то, или
после устранения детерминированных дефектов CU становится СТАБИЛЬНО зелёным.

## НАХОДКА №3 (текущий блокер) — CC-FAIL: undefined reference to `uv_strerror`

Первый живой end-to-end прогон `nova test
spec_tests/conformance/app_effect_basic_t8_1.nv` (rt_archive default ON,
`Toolchain: clang`) — **CC-FAIL** на LINK-этапе (не RUN-FAIL!):
```
undefined reference to `uv_strerror'
```
(`fibers.h:3692,3699,3876` — `_nova_sleep_via_libuv`/`nova_blocking_offload`).
Символ РЕАЛЬНО есть в `libuv.a` (`nm` подтвердил). Похоже на LINK-ORDER
баг: `build_command` (`test_runner.rs:1478-1521`, Linux `#[cfg(target_os =
"linux")]` ветка) оборачивает `--start-group libuv.a <syslibs>
--end-group`, но `opts.c_file` + `rt_archive.lib_file` (или individual
rt_*.c включая `rt_fibers`/`rt_driver`, которые реально зовут
uv_strerror) добавляются В КОМАНДУ ПОЗЖЕ (строки 1576-1593) — т.е. ПОСЛЕ
закрытия group. Стандартное правило GNU ld: архив резолвит символы,
undefined НА МОМЕНТ его появления в command-line; символы, ставшие
undefined ПОЗЖЕ (из объектов после архива), НЕ ищутся повторно ни в
`libuv.a`, ни внутри УЖЕ закрытого `--start-group`. **Нужно перепроверить
эмпирически**: пока НЕ подтверждено детерминированностью (второй прогон
запущен, background) — TODO следующим шагом.

**Подтверждено вторым прогоном (2/2 идентично):**
- `NOVA_RT_ARCHIVE=1` (default): **CC-FAIL**, `uv_strerror` — 100%
  детерминированно, идентичный текст оба раза.
- `NOVA_RT_ARCHIVE=0`: **RUN-FAIL** — процесс завершается ЧИСТО (без
  SIGSEGV/крэша), детальная строка:
  ```
  FAIL: f64 full math-intrinsic set resolves + computes (D109/D74 wave-2)
    — app_effect_basic_t8_1.nv:22: assert failed: 27.0.cbrt() == 3.0
  | FAIL: f32 full math-intrinsic set resolves + computes (D109 wave-2,
    previously uncovered) — app_effect_basic_t8_1.nv:22: assert failed:
    twentyseven.cbrt() == 3.0
  ```
  ОБЕ FAIL-строки показывают line=22 (Находка №1's предсказание для ЭТОГО
  CU — 21 '\n' в entry-файле → line=1+21=22 — подтверждено ТОЧНО, для ДВУХ
  разных реальных строк d109 — 24 и 56). **PASS:0 FAIL:1, БЕЗ КРЭША** —
  процесс НЕ падает на выходе в этом прогоне (расходится с историч.
  сигнатурой "падает на выходе"/SIGSEGV — либо та гонка УЖЕ закрыта
  прошлыми волнами [211/187w], либо нужно больше прогонов чтобы поймать
  остаточную редкую гонку).

**ФИКС применён (test_runner.rs, обе ветки Clang+Gcc, `build_command`):**
разделил единый libuv-блок на (1) defines+include (`-DNOVA_USE_LIBUV=1`,
`-I libuv_include`) — оставлены на исходном раннем месте (order-independent
для препроцессора); (2) объекты/библиотека (`rt_net.c`/`rt_fs.c` non-archive
+ `libuv.a`/syslibs, `--start-group`/`--end-group` на Linux) — ПЕРЕНЕСЕНЫ
после добавления `opts.c_file` + rt_archive.lib_file/individual rt_*
sources. Windows-ветка (`#[cfg(target_os="windows")]`) оставлена на
исходном раннем месте БЕЗ ИЗМЕНЕНИЙ (MSVC linker не имеет этого
order-ограничения, ноль риска регрессии). GCC-ветке ДОПОЛНИТЕЛЬНО добавлен
`--start-group`/`--end-group` (которого не было вообще) — консистентно с
Clang-веткой, тот же ld. `cargo check` Windows — чисто (0 новых warnings).
Пересборка на WSL — в процессе.

**Фикс cbrt (spec_tests fixture, findings #2 отдельно задокументированы
выше):** оба assert'а (`d109_primitive_methods_f64_f32_math.nv:24,56`)
переведены с exact equality на epsilon-сравнение (`1e-9`/`1e-5`
f64/f32) — платформенно-портируемо, сохраняет намерение теста.

**НЕ исправлено в этой волне (out-of-zone, задокументировано, не тронуто):**
Находка №1 (loc_for_span/`self.source_file_name` — process-global, не
per-span originating file) — корень требует per-declaration file provenance
в AST (Span сейчас = только {start,end} без file_id; `TestDecl`/`Item` не
хранят originating file) — архитектурная правка ВНЕ `nova_rt`+`test_runner.rs`
зоны этой волны, нужен отдельный заход с решением владельца (как минимум:
добавить file_id в Span ИЛИ per-item side-table, прокинуть через
folder-module merge в imports.rs). Новый backlog-маркер нужен:
`[M-emit-c-loc-for-span-wrong-file-merged-cu]`.

## Прогресс фиксов (после первой пересборки)

- `uv_strerror` link-order фикс — ПОДТВЕРЖДЁН (симптом исчез после
  пересборки, `NOVA_DEBUG_CC_DUMP=1` дамп показал ДРУГОЙ (следующий)
  undefined-symbol, не uv_strerror — значит первый фикс реально резолвит
  то, что чинил).
- Добавлен env-gated `NOVA_DEBUG_CC_DUMP=1` (test_runner.rs, рядом с
  существующим `NOVA_DEBUG_TIMEOUT_DUMP`) + расширен `errs`-фильтр
  (раньше матчил только substring "error" — GNU ld'шные `undefined
  reference to` НЕ содержат слово "error", фильтр давал только
  clang-обёртку "linker command failed", теряя реальный символ). Это
  ПРЯМОЕ улучшение label/attribution-инфраструктуры (гипотеза б —
  "test-harness label до, не после" из задания).

## НАХОДКА №4 — rt-archive-build пропускает `-ffunction-sections`/`-fdata-sections` (Unix)

После фикса #3, следующий `NOVA_DEBUG_CC_DUMP=1` прогон с rt_archive=1
показал: `undefined reference to _nova_bench_heap_sample_interval_ns` /
`_nova_bench_heap_sampler_stop` (`bench.h:320,327`, внутри
`nova_bench_heap_sampler_thread`, статическая ф-ция, включена
безусловно при `NOVA_USE_LIBUV`). Эти два extern-глобала ОПРЕДЕЛЯЮТСЯ
ТОЛЬКО макросом `NOVA_BENCH_STATE_DEFINE`/`NOVA_BENCH_HEAP_SAMPLER_THREAD_DEFINE`,
который codegen эмитит `if self.bench_mode` (emit_c.rs:7174-7178) —
**НИКОГДА для обычного `nova test`/`nova build`**. Раньше (per-build
inline compile, pre-218) это не всплывало, потому что `build_command`
(главный путь) компилирует с `-ffunction-sections -fdata-sections` +
линкует с `-Wl,--gc-sections` — мёртвая `nova_bench_heap_sampler_thread`
(никогда не вызывается вне bench_mode) вычищается ЛИНКЕРОМ на уровне
секций ДО того, как её неразрешённые ссылки становятся проблемой.
**`build_rt_archive_lib`'s Unix-ветка (test_runner.rs:5311-5362,
компилирует .c → .o для архива) НЕ передавала `-ffunction-sections
-fdata-sections`** — без per-function секций `--gc-sections` может
выкинуть архивный `.o`-член ТОЛЬКО целиком (не может), а раз effects.o/
runtime.o и т.п. и так нужны (другие символы оттуда используются),
мёртвая ф-ция линкуется вместе с остальным содержимым `.o` — тянет за
собой неразрешённые ссылки. Windows-половина той же функции уже имела
эквивалент (`/Gy`, function-level linking, строка `lib_cmd`/compile.rsp) —
асимметрия Unix/Windows, не архитектурная гонка.

**Фикс:** добавлены `-ffunction-sections`/`-fdata-sections` в
`build_rt_archive_lib`'s Unix compile-loop (test_runner.rs) — доводит
Unix до паритета с Windows-половиной И с главным (non-archive)
build-путём. Пересборка/повторный прогон — в процессе.

## Следующий шаг

Пересобрать (2-й фикс) на WSL, повторить прогон `app_effect_basic_t8_1`
с rt_archive default ON (реальный CI-путь). Ожидание: CC-FAIL исчезает
(оба undefined-symbol класса резолвятся), cbrt FAIL исчезает (epsilon),
итог — PASS 1/0 стабильно. Затем ×10-20 повтор на представителе (ловить
остаточную M:N-гонку, если есть) + Windows regression (standalone CU +
флагман) + финальный вердикт.

**Оба фикса подтверждены:** пересборка чистая, изолированный прогон с
rt_archive default ON — **PASS: 1 FAIL: 0**. Windows-регрессия зелёная:
`standalone` CU 70/0; `pos_max_fibers_concurrent`+`supervisor_stop_test`+
`supervisor_parfor_test` ×5 — все PASS; флагман-агрегатор собрался
(`--strict-effects`) и ответил HTTP 200 на живой curl.

## ИТОГОВЫЙ ГЕЙТ — ×20 WSL, rt_archive default ON (реальный CI-путь)

**20/20 PASS** (`run 1` … `run 20`, все `PASS: 1  FAIL: 0`, `ALL_DONE`) —
представитель `app_effect_basic_t8_1` (весь 993-файловый folder-CU) собран
и прогнан заново 20 раз подряд, ноль отклонений, ноль крэшей/SIGSEGV/
зависаний. Никакой остаточной M:N-гонки не поймано — вердикт: то, что
исторически репортилось как "[M-linux-mn-conformance-red] Linux M:N
RUN-FAIL флака", было ТРЕМЯ детерминированными дефектами (Находки №2-4
выше), не гонкой. known_red-строка в `.github/workflows/nova-gate.yml`
СНЯТА (заменена на подробный closure-комментарий с диагнозом+фиксом+числами,
`if [ "${code}" -ne 0 ]` теперь без masking-логики — любой FAIL красный).

## Вердикт по гипотезам задания

- **(а)** cbrt-assert падает РЕАЛЬНО — но НЕ из-за гонки, а из-за
  платформенной (glibc/libm) погрешности округления, деterministически на
  clang-Linux (Находка №2). Подтверждено.
- **(б)** attribution — ПОДТВЕРЖДЕНО, но иначе, чем предполагалось: не
  test-harness (test_runner.rs), а КОМПИЛЯТОРНЫЙ баг
  `emit_c.rs::loc_for_span` (process-global `source_file_name` вместо
  per-span originating file) — 100% детерминированная мисатрибуция для
  ЛЮБОГО assert/contract-fail в non-entry peer-файле folder-module CU
  (Находка №1, задокументирована, НЕ починена — вне зоны, архитектурная
  правка AST). Плюс УЛУЧШЕНА диагностика test_runner.rs (`NOVA_DEBUG_CC_DUMP`,
  расширенный errs-фильтр) — прямое попадание в "test-harness label
  до, не после".
- **(в)** память/GC-гонка портит данные d109-теста — ОПРОВЕРГНУТО. Assert
  падает ВСЕГДА с ОДНИМ И ТЕМ ЖЕ значением (3.0000000000000004441), не
  случайным мусором — чистая детерминированная числовая проблема, не
  порча памяти.

## Файлы, затронутые этой волной

- `compiler-codegen/src/test_runner.rs` — link-order фикс (Clang+Gcc Unix
  branches), `-ffunction-sections`/`-fdata-sections` в `build_rt_archive_lib`
  Unix-ветке, `NOVA_DEBUG_CC_DUMP` + расширенный errs-фильтр.
- `spec_tests/conformance/d109_primitive_methods_f64_f32_math.nv` — 2 cbrt
  assert'а на epsilon-сравнение.
- `.github/workflows/nova-gate.yml` — known_red-масштаб снят, closure-
  комментарий.
- `docs/plans/wip/linux-mn-red-notes.md` (этот файл) — чекпоинт.

Новый backlog-маркер нужен интегратору: `[M-emit-c-loc-for-span-wrong-file-merged-cu]`
(Находка №1, компиляторный баг, НЕ починен в этой волне — вне зоны/бюджета,
нужна per-declaration file provenance в AST).

Модель: sonnet.

## Методологическая находка (важно для следующих агентов) — multi-line-скрипт через wsl.exe теряет newline при auto-backgrounding

При запуске ДЛИННОГО (>120с) `wsl.exe -d Ubuntu -- bash -c '<многострочный
скрипт>'` через Bash-тул, если команда авто-переходит в background
(таймаут), ВНУТРЕННИЕ переводы строк многострочного `bash -c '...'`
аргумента иногда СХЛОПЫВАЮТСЯ В ПРОБЕЛЫ при повторной сериализации для
фонового выполнения (подтверждено `ps aux` дампом: весь скрипт стал ОДНОЙ
строкой через пробелы). Результат: `cd /home/craft/nova-appeffect-wsl`
никогда не выполнялся как ОТДЕЛЬНАЯ команда (стал лишним аргументом `rm
-f`), процесс наследовал WSL-дефолтный cwd = `/mnt/d/Sources/nv-lang/nova`
(транслируется из Windows-cwd вызывающего git-bash) — **тестировал
СОВСЕМ ДРУГОЕ ДЕРЕВО** (оригинальный D:-репозиторий через медленный 9p-mount,
БЕЗ моих фиксов в фикстуре) — объясняет и аномальную медлительность
(4-5 минут на прогон вместо ожидаемых ~30-60с), и риск ложных
результатов. **Диагностировано через `readlink /proc/<pid>/cwd` +
`cat /proc/<pid>/environ | grep PWD`** — несовпадение с ожидаемым cwd
было прямой уликой. **Обход:** писать многострочный скрипт В ФАЙЛ
(`Write` на Windows-стороне → `cp` в WSL native fs) и запускать
`wsl.exe -d Ubuntu -- bash /home/craft/script.sh` — короткая
командная строка, риска схлопывания многострочности нет (подтверждено:
`readlink /proc/<pid>/cwd` после фикса = ожидаемый `/home/craft/
nova-appeffect-wsl`). Короткие однострочные/через `&&` команды (сборки,
единичные прогоны) фиксацию НЕ ловили — проблема специфична для
длинных многострочных heredoc-style скриптов, уходящих в background.
