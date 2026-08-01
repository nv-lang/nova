<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Backlog — project-wide OPEN followup markers (`[M-…]`)

> **Роль.** Единый **OPEN-view** — что из `[M-…]`-followup'ов **реально открыто** прямо сейчас
> (actionable «что живо»), по всему проекту. Каждая строка указывает свой **home** (план или floating).
>
> **Чем НЕ является.** Это не история закрытых упрощений — с 2026-07-18 (чистка) закрытое живёт в
> [`docs/history/simplifications-closed.md`](../history/simplifications-closed.md); в
> [`docs/simplifications.md`](../simplifications.md) — только ДЕЙСТВУЮЩИЕ упрощения. Backlog = только
> живой OPEN-срез + индекс. Детали plan-bound маркера живут в Followups его плана; здесь — индекс с home.
>
> **Lifecycle (для агентов):**
> 1. Новый floating-маркер → **добавить строку сюда** + залогировать в `simplifications.md` (house style).
> 2. Маркер сделан/закрыт → **убрать строку отсюда** (история остаётся в `simplifications.md` + commit). Держим OPEN-view коротким — только живое.
> 3. Маркер дорос до своего плана → перенести в Followups плана (здесь оставить индекс-строку с home).
> 4. Перед работой над смежной подсистемой — **заглянуть сюда**.
>
> Конвенция: [AGENTS.md → Followup markers](../../AGENTS.md).
> **Создан/выверен:** 2026-06-11 (триаж 58 OPEN-tagged → 24 really-open + 34 stale; workflow w33ant6rp).

---

## P1 — вне объёма Plan 208 Ф.4R, найдено попутно (блокирует мега-CU гейт)

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-forin-crosspkg-char-to_str-blanket-collision]` | **✅ РЕШЕНО 2026-07-21 (findings A-S4, worktree `nova-forin`, ветка `p-fix-forin-crosspkg`, sonnet).** Задача была сформулирована как «for-in cross-package resolve gap» (nova-http 5×CODEGEN-FAIL «for-in: cannot resolve iterator type for expression of C-type 'nova_int'», все 5 CU импортируют `compress` напрямую/транзитивно) — глубокое расследование (byte-offset span tracing через `DEBUG span={:?}` в `emit_for`'s error-arm + инструментированный `infer_expr_c_type`/`infer_call_ret_c`) показало, что **корень НЕ cross-package**: реальный failing for-in — `std/src/encoding/url.nv:519` (`http`-пакета root-peer файл, БЕЗ compress) `for b in c.to_str().bytes()` внутри `for c in s.chars()`. Бисект подтвердил репро сохраняется даже с `import compress` полностью вырезанным из `client.nv` (копия nova-http в scratch, compress-branches застублены) — тот же CODEGEN-FAIL. Настоящий корень: `char @to_str()` резолвится ТОЛЬКО через bare-T blanket (`fn[T] T @to_str() -> str`, std/runtime/string/core.nv) — ни `resolved_callees` (Channel 1), ни `resolved_types` (Channel 2) не несут аннотацию для этого call (checker резолвит blanket без записи в codegen-читаемые каналы). `infer_call_ret_c`'s legacy fallback (`compiler-codegen/src/codegen/emit_c.rs`, ~54105, помечен как «name-only key is last-wins across types» — уже задокументированный класс) читает `var_types["fn_ret_to_str"]` — ОДИН флэт-ключ БЕЗ receiver-типа, куда forward-decl пишет ЛЮБОЙ КОНКРЕТНЫЙ `to_str`-метод (generic-blanket методы делают ранний return в forward-decl pass и НИКОГДА не пишут в этот ключ, ~14778-14811) — единственный писатель этого ключа в CU оказался `[]u8 @to_str() -> Result[str, Utf8Error]` (Plan 174.1/196.7). Итог: `char.to_str()` инферился как `Result[str, Utf8Error]` вместо `str`, `.bytes()` на этом эрродировал до `nova_int`, for-in терял резолв iterator-типа. Родственный, но ОТДЕЛЬНЫЙ баг от уже закрытого чекер-side `[M-char-blanket-shadowed-by-sig-complete]` (std/src/runtime/char_test.nv) — тот про SIG-COMPLETE overload-gate чекера, этот — про codegen-side name-only return-type fallback. **Фикс** (`emit_c.rs`, ~54105, перед `B11af_fn_ret_method_nameonly`): для ПРИМИТИВНОГО receiver'а (char/int/bool/f32/f64/byte/str/u8..u64/i8..i64) codegen сначала ищет bare-T blanket в `mono_method_decls[(1-2-uppercase-letter-tvname, method)]` (зеркалит уже существующий Plan-161 гейт `~52726`, который делает то же самое, но был ограничен mono/`type_impl_protocols` ресиверами, НЕ примитивами) — и только если blanket не найден, падает на старый name-only fallback. GATED так, что не-примитивный/уже-type-qualified путь остаётся byte-identical. Регресс-фикстура: `std/src/runtime/char_test.nv` (новый тест `"char.to_str().bytes() nested for-in — no []u8.to_str() name collision"`, репро точной формы из url.nv, PASS). Гейты: `nova-http` `nova test src --jobs 1/16×3` — все 5 for-in CODEGEN-FAIL исчезли (2 PASS + 1 SKIP, детерминированно); честный остаток — 3 CC-FAIL `Nova_ErrorKind` undeclared (см. отдельный маркер `[M-http-compress-errorkind-crosspkg-collision]` ниже, ДЕЙСТВИТЕЛЬНО cross-package, но другой баг); `std/src/collections` PASS 13/FAIL 0/SKIP 6; `std/src/runtime` PASS 5/FAIL 0/SKIP 13 (включая новый тест); флагман `examples/flagship/aggregator --strict-effects` собран чисто (тянет http-путь-зависимостью). Зона правки: только `compiler-codegen/src/codegen/emit_c.rs` (`infer_call_ret_c`) + новый тест в `std/src/runtime/char_test.nv`. | floating (codegen, emit_c.rs) | **✅ РЕШЕНО** |
| `[M-git-cache-resolve-jobs-race]` | **✅ РЕШЕНО 2026-07-21 (findings A-S4, Task 2, worktree `nova-forin`, ветка `p-fix-forin-crosspkg`, sonnet).** `nova test --jobs N` (N>1) на пакете с git-зависимостью (`version`/`branch`-пин, где `resolve_git_dep_in` делает `git fetch` на КАЖДЫЙ резолв, не только на холодном кэше) интермиттентно давал «fetch git-зависимости ... importing file: error.nv» на случайных CU. `--jobs N` — N ПОТОКОВ в ОДНОМ процессе (`nova-cli/src/main.rs`), не отдельные процессы. Root cause — classic check-then-act race в `compiler-codegen/src/git_cache.rs::resolve_git_dep`: `memo().lock().unwrap().get(&key)` (короткий лок, немедленно освобождается) → git-работа (`clone`/`fetch`/`worktree add` — МЕДЛЕННО, БЕЗ лока) → повторный лок только для `insert`. Два потока, резолвящие ОДИН И ТОТ ЖЕ `(url, pin, locked_commit)` одновременно, оба видят memo-miss и ОБА гоняют `git fetch`/`git worktree add` на ОДНОМ bare-repo каталоге конкурентно — гонка на `.git`-директории / конфликтующий `worktree add` на тот же commit. Фикс: `lock_for_key` — per-key `Arc<Mutex<()>>` (таблица `key_locks()`, запись создаётся под коротким локом внешней таблицы, затем сразу освобождается), захватывается на ВЕСЬ check-memo→git-work→insert-memo критический участок в `resolve_git_dep`. Разные ключи (разные `url`/`pin`) резолвятся полностью параллельно — сериализуются только гонящиеся за ОДНИМ и тем же ключом (второй в очереди получает дешёвый memo/cache-hit вместо дублирования git-работы). Регресс-тест: `compiler-codegen/src/git_cache.rs::tests::concurrent_resolve_same_key_no_race` — 16 потоков, ОДИН `(url, Tag)` через ПУБЛИЧНЫЙ `resolve_git_dep` (с реальным `NOVA_HOME`-кэшем, не test-only `resolve_git_dep_in`-обход) — все 16 успешны, все возвращают идентичный commit/checkout. Гейты: `cargo test --release --lib git_cache` — 8/8 PASS (7 существующих + новый); `nova-http` `nova test src --jobs 16` ×3 — ноль git-fetch/clone/worktree ошибок (детерминированный тalli PASS 2/CC-FAIL 3(ErrorKind, см. отдельный маркер)/SKIP 1 все 3 прогона); отдельно — прогон с ХОЛОДНЫМ `NOVA_HOME` (fresh clone+fetch конкурентно 16-ю потоками одновременно для compress+tls) — ноль git-ошибок (упёрлось в НЕсвязанный vendor-FFI mbedtls/brotli lib-build артефакт в sandbox, не git). Зона правки: только `compiler-codegen/src/git_cache.rs` (новые `key_locks`/`lock_for_key` + `resolve_git_dep` держит per-key-лок через всю секцию) + новый unit-тест. | floating (git_cache, test_runner concurrency) | **✅ РЕШЕНО** |
| `[M-http-compress-errorkind-crosspkg-collision]` | **✅ РЕШЕНО 2026-07-21 (worktree `nova-errkind`, ветка `p-fix-errorkind-crosspkg`, sonnet).** Найдено при закрытии `[M-forin-crosspkg-char-to_str-blanket-collision]` — 3 CC-FAIL nova-http сьюта (`src/client/client`, `src/serdejson/serdejson`, `src/transport/real`): `clang: error: use of undeclared identifier 'Nova_ErrorKind'` на `_nv_scr_NNNN->kind`. Расследование (byte-offset trace через инструментированные `resolved_named_to_c`/`ref_type_base`/`emit_type_decl`/`drain_generic_type_worklist`/`emit_match`/`infer_expr_c_type`) ОПРОВЕРГЛО первоначальную гипотезу «D381 сканирует только один пакет»: `module.peer_files` уже ЕДИНЫЙ список на весь CU (включает http+compress+std.io), и D381's collision-scan УЖЕ качественно детектирует и квалифицирует все три `ErrorKind` (`Nova_http_ErrorKind`/`Nova_compress_ErrorKind`/`Nova_std_io_ErrorKind`) на большинстве сайтов. Настоящий корень — ТРИ узких пробела в самом уже-существующем механизме (НЕ новая архитектурная ось): (1) два module-level pre-scan'а без file-context-гейта — D84 free-fn overload-регистрация (шаг «1c») и `user_fn_sigs` sig-preseed («B10f») — вызывали `type_ref_to_c` на КАЖДУЮ free-fn сигнатуру (`HttpError.new(kind ErrorKind)`) ДО того, как per-fn `current_emit_file_id`-гейт хоть раз срабатывал для этого элемента (контекст `None`/stale); аналогично `emit_generic_type_instance_scoped_inner` (мономорфизация из `drain_generic_type_worklist`) не гейтировала file-context от `template.span.file_id`. (2) `ref_type_base`'s REF-резолюция проверяла ТОЛЬКО собственные imports файла — но folder-модуль (D29/D78/D281) это ОДНО пространство имён на несколько co-equal файлов, и test-файл (`client_test.nv`) может ссылаться на `ErrorKind` СТРУКТУРНО (через уже-импортированный `HttpError.kind`) без собственного `import http.{ErrorKind}`, пока СОСЕДНИЙ файл группы (`client.nv`) его импортирует — добавлен sibling-scan fallback (условие 3) по всей `effective_modpath`-группе. (3) когда НИКТО в группе не импортирует имя вовсе (чисто структурная ссылка), checker-канал (`resolved_types` → Channel 2 в `infer_expr_c_type`) резолвит ГОЛОЕ имя поля (`ErrorKind`, `module: []`) — не несёт DEF-модуль; но `record_schemas[<Struct>][<field>]` (зарегистрирована при эмите САМОГО типа, ВСЕГДА под D381-гейтом на своём объявляющем файле) уже несёт корректно квалифицированную строку — добавлена узкая REF-поправка: Channel 2 Member-ветка предпочитает схему, когда канал вернул НЕКВАЛИФИЦИРОВАННОЕ коллидирующее имя, а схема несёт другое (расхождение возможно только когда `colliding_type_names` уже непусто — не-коллидирующий CU не задет). D-амендмент: `spec/decisions/08-runtime.md` D381-блок (детали трёх пробелов + единая-точка не расширена, приоритет между уже существующими источниками истины). Гейты: `nova-http` `nova test src --jobs 1` ×2 — **PASS 5/FAIL 0/SKIP 1** (детерминированно, `servernet` SKIP честный); `nova-tls`/`nova-compress` сьюты не сломаны; флагман `examples/flagship/aggregator --strict-effects` (тянет http+tls+compress через `examples/nova.toml`) собран чисто; standalone 15 conformance-фикстур (enum/sum) без CC/CODEGEN-регрессии (SKIP по отсутствующему `NOVA_SMT_BACKEND`, не связано). Зона правки: только `compiler-codegen/src/codegen/emit_c.rs` (`ref_type_base` sibling-scan; step-1c/B10f pre-scan file-context гейт; `emit_generic_type_instance_scoped_inner` то же; `infer_expr_c_type` Channel-2 Member schema-preference). ABI/`#repr`/extern-типы НЕ задеты (то же самое D381-порождённое имя распространено на ранее пропущенные codegen-сайты, схема мангла не изменена). | floating (codegen, emit_c.rs D381 gap-close) | **✅ РЕШЕНО** |
| `[M-d216-write-at-return-type-unknown-cc-panic]` | **ЗАКРЫТ 2026-07-21 (worktree `nova-d216fix`, ветка `p-fix-d216-write-at`, sonnet).** Корень: `infer_expr_c_type`'s `ExprKind::Block`-арм (`compiler-codegen/src/codegen/emit_c.rs`, ~строка 54543) при пробе типа trailing-выражения блока (для void-cast решения после реальной эмиссии) НЕ пре-регистрировал top-level `let`-локалы САМОГО блока — если trailing ссылался на такой локал как РЕСИВЕР метод-вызова (`unsafe { mut q = buf.ptr(); q.write_at(1, 99) }`), вложенный Ident-лукап внутри `infer_call_ret_c`/`recv_c_type_materialized` не находил `q` → `obj_ty=""` → `[P67-LEGACY]` паника. Второй слой (вскрылся после первого фикса на `.copy_to`/`s`, `.copy_from`/`d`): `var_types` — плоский, НЕ block-scoped реестр; в нём уже могли лежать ЧУЖИЕ stale-записи под тем же коротким именем (`d`/`s`) от std/prelude-функций, собранных ранее в том же CU — наивный overlay с гвардом `!contains_key` ошибочно считал такую stale-запись авторитетной и пропускал установку оверлея. Фикс: безусловный overlay через `pattern_binding_overrides` (RefCell, т.к. функция `&self`) — пре-регистрировать тип каждого top-level `let` перед пробой trailing, пробовать, восстанавливать; тот же паттерн, что уже используется у `Match`-арма `block_saved` (~55281) и у let-RHS body-bearing overlay (~27805). Коммит `75ea680f5` (ветка `p-fix-d216-write-at`, не смёржена в main — интегратор заберёт). Точечные репро зелёные: изолированный `d216_ptr_methods_174_5.nv` (сам по себе и по реальному пути внутри `spec_tests/conformance`, что триггерит тот же folder-module merge) — PASS; `d432_auto_cleanup_hybrid_c.nv` (независимый репро того же класса паники) — PASS. Полный мега-CU `spec_tests/conformance` гейт — за интегратором (не гонялся в этой волне по решению координатора: фон умирает с завершением хода агента). | floating (не привязан к плану) | closed |
| `[M-a-q3-cmd-build-missing-autoderive-inject]` | **ЗАКРЫТ 2026-07-21 (worktree `nova-dbgrec`, ветка `p-fix-println-debug-record`, sonnet).** Находка A-Q3 tour-волны: `#impl(Debug)` auto-derive record, `println("${p:?}")` — под standalone `nova build`+run печатал МУСОР (сырой указатель, cast в int) вместо `TypeName { field: value }`, при этом байт-идентичный код внутри `test {}` (через `nova test`) печатал корректно. Корень — НЕ codegen-диспетч (`emit_c.rs::emit_interpolated_str`, дошёл до `nova_int_to_str((nova_int)(v))` last-resort только КАК СЛЕДСТВИЕ), а `nova-cli/src/main.rs::cmd_build`: вызывался ТОЛЬКО serde-фильтрованный `inject_synthesized_methods_filtered` (pre-check) — НЕ-serde вариант (Plan 126.2 Ф.2, инжект Equal/Hash/Clone/Compare/Display/Debug auto-derive `FnDecl` в `module.items`) в `cmd_build` отсутствовал вовсе; `test_runner.rs` (пайплайн `nova test`) этот вызов уже имел (после effects/lints, до annotate-maps/desugar) — отсюда рассинхрон путей. Фикс: добавлен недостающий `inject_synthesized_methods(&mut module)` в `cmd_build` на ТОЙ ЖЕ позиции, что и в `test_runner.rs`. Коммит `30babe0e4` (ветка `p-fix-println-debug-record`, не смёржена в main — интегратор заберёт). Новая conformance-фикстура `spec_tests/conformance/a_q3_println_debug_record.nv` (фиксирует derive/dispatch-поведение, но НЕ может проверить именно wiring `cmd_build` — conformance всегда идёт через `nova test`, см. заголовок фикстуры). Гейты: изолированный репро RED→GREEN (`nova build`+run, standalone); `nova test` того же кода — зелёный (был зелёным и до фикса, не regression); 5 d422-фикстур (baseline float/int/strcharboolu64 + generic_container_derive + unified_display_dispatch) PASS; `fmt_buf/core_test` + `string_builder_test` PASS; 11 interp/debug conformance-фикстур (pos_impl_debug, contract_msg_interp_pos, d194_debug_contract_prefix_pos, d229_debug_format_spec, d422_generic_interp_dispatch, ensures_msg_interp_pos, f2_contract_msg_interp_dce, invariant_msg_interp_pos, pos_option_debug, pos_result_debug, debug_single_fail) PASS; `examples/flagship/aggregator --strict-effects` собран чисто. Полный мега-CU `spec_tests/conformance` гейт — за интегратором. | floating (не привязан к плану) | closed |

## P2 — найдено владельцем (trap-test lane), закрыто в той же волне

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-trap-tests-silent-skip-default-lane]` | **✅ РЕШЕНО 2026-07-21 (worktree `nova-trapnames`, ветка `p-fix-trap-test-lane`, sonnet).** Владелец нашёл: `std/src/time/rt/{dur_f64_nan_traps,dur_div_zero_traps,dur_add_overflow_traps}.nv` (EXPECT_RUNTIME_PANIC trap-тесты, D317-паритет) под дефолтным `nova test std/src/time/rt` репортили голый «PASS: 0 FAIL: 0» без единой SKIP-строки — неотличимо от пустой/опечатанной директории. Корень: `walk_nv_selected` (`compiler-codegen/src/test_runner.rs`) молча ронял файл из `out`, если его `EXPECT_*`-тип (или `_slow`-суффикс) не входит в активную `TestSelection` (default = `{Positive}`, без `--include-slow`) — ни один счётчик, ни одна строка вывода не фиксировали сам факт исключения. Фикс: новый `LaneExclusion` enum (`Type(TestType)` / `Slow`) + `walk_nv_selected_ex` (собирает исключённые файлы с причиной, `walk_nv_selected` — тонкий wrapper для обратной совместимости) + `SkipReason::LaneExcluded { lane, hint }`; `run_all` синтезирует precomputed `Outcome::Skipped` для каждого lane-исключённого файла и проводит его через тот же job-конвейер (те же `--filter`/`--skip`/`--filter-from`, БЕЗ codegen/cc/run) — итог: видимая строка `SKIP <path> # <lane> lane — requires <hint>` в общем `SKIP:`-счётчике. Проверены и НЕ тронуты (сознательно, другая категория): OS-suffix mismatch (`_windows.nv` и т.п. — platform-gating, шумело бы по всему std/spec_tests) и `EXPECT_TRAP` (маркер не существует в кодовой базе — ложный след). Дополнительно: переименованы 4 std-side legacy `EXPECT_RUNTIME_PANIC` файла с голым множественным числом `*_traps.nv` (ничем не сигналившим «тест-файл») в `<scenario>_trap_test.nv` (git mv + module-декларатор под D78: для standalone-файла target = ПОЛНОЕ имя файла, `_test` не отрезается) — `std/src/time/rt/{dur_f64_nan,dur_div_zero,dur_add_overflow}_trap_test.nv` + `std/src/time/civil/rt/date_period_overflow_trap_test.nv` (4-й найден тем же грепом `^test "` без `_test` вне `neg/` по всему std; ещё 3 находки грепа — `fmt_buf/core.nv`/`testing/property.nv`/`time/duration/core.nv` — легитимный «inline тесты в std»-паттерн, не тронуты). `docs/test-conventions.md` — новый параграф про `_trap_test.nv`-нейминг + напоминание про `--full`/`--include-slow` lane. Гейты: `nova test std/src/time/rt` (default) — **3 видимых SKIP** (`PASS:0 FAIL:0 SKIP:3`); то же с `--full` — **3/3 PASS**; `nova test std/src/time/civil/rt` (default) — 1 SKIP, `--full` — 1/1 PASS; `cargo test --lib test_runner` (искл. pre-existing `p0_erased_now_dispatches_via_vtable` stack-overflow, не связан с правкой) — 55/55 PASS, включая 2 новых теста (`walk_nv_selected_ex_reports_excluded_lanes`, `lane_excluded_skip_reason_description`); `nova build examples/flagship/aggregator/src/main.nv --strict-effects` — собран чисто. Зона правки: `compiler-codegen/src/test_runner.rs`, 4 переименованных `.nv`-файла, `docs/test-conventions.md`. Коммиты: `f45c870ee` (test_runner.rs SKIP-видимость), `ce8eec72e` (rename + D78 + конвенция). | floating (test_runner.rs + std trap-tests naming) | **✅ РЕШЕНО** |

## P2 — вне объёма Plan 219, найдено попутно

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-218-rt-archive-parallel-jobs-race]` | **✅ РЕШЕНО 2026-07-20 (worktree `nova-218race`, ветка `p-fix-218-archive-race`, sonnet).** Root cause подтверждён репродюсом: `nova test --jobs N` — это N ПОТОКОВ внутри ОДНОГО процесса (`std::thread::scope`, `test_runner.rs` ~5983), не отдельные OS-процессы. `detect_or_build_rt_archive` отпускал `RT_ARCHIVE_MEMO`-лок МЕЖДУ проверкой и сборкой — на холодном bucket'е несколько потоков одновременно проходили memo-miss + `lib_file.is_file()==false` и все шли в `build_rt_archive_lib` на ОДИН `cache_dir` (общий `obj/`, общие `.rsp`, общий финальный `lib_file` без атомарности записи) — отсюда флейк-`CC-FAIL` (архив то отсутствует, то повреждён в момент линковки). PRE-FIX репро (`--jobs 4`, чистый кэш, x10, батч `spec_tests/conformance/neg/f*.nv`): `build_triggers=4` на КАЖДОЙ итерации (все 4 воркера реально строили), FAIL получен на итерации 8. Соседний `detect_or_build_libuv` НЕ образец атомарности — он просто зовётся ОДИН раз до старта воркер-пула, гонки в принципе нет; реальный образец — `nova-cli/src/build_cache.rs::store_c` (temp-файл + `fs::rename`). Фикс, два слоя в `test_runner.rs`: (1) `detect_or_build_rt_archive` — ОДИН `MutexGuard` держится через всю check→build→memoize последовательность (был отпущен и взят заново) → первый поток строит, остальные ждут на мьютексе (побочный эффект: `build_triggers=1` вместо 4, никакого дублирования CPU); (2) `build_rt_archive_lib` — сборка в УНИКАЛЬНЫЙ scratch-каталог (`unique_build_tag()`: pid+counter+nanos), финальный `lib_file` публикуется ОДНИМ atomic `fs::rename` в конце (defense-in-depth для отдельных `nova`-процессов, которые лок из (1) не видит). POST-FIX repro: тот же прогон x10 — `f1_parse_message_positive` PASS все 10 раз, `build_triggers=1` стабильно. std/checksums+collections `--jobs 4` x5 — `PASS:16 FAIL:0` все 5. Standalone-CU (`spec_tests/conformance/neg`, 406 файлов, `--full --jobs 4`, тёплый кэш): `PASS:405 FAIL:0 SKIP:3`. 218-выигрыш сохранён: тёплый кэш не печатает rebuild-сообщение (fast-path `is_file()`), архивный путь стабильно быстрее inline-пути (`NOVA_RT_ARCHIVE=0` сравнение). Детали — [docs/plans/wip/218-race-notes.md](wip/218-race-notes.md). Зона правки: только `compiler-codegen/src/test_runner.rs`. | Plan 218 (`compiler-codegen/src/test_runner.rs`) | **✅ РЕШЕНО** |

## P2 — вне объёма Plan 202, найдено попутно

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-202-ident-x-module-alias-collision]` | **✅ РЕШЕНО 2026-07-20 (worktree `nova-identx`, ветка `p-fix-ident-x`, sonnet).** Root cause НЕ подтвердил исходную гипотезу (auto_derive.rs) — `auto_derive.rs` везде использует префиксованные синтетические имена (`__nv_ta`/`__nv_cmp_N`/`__nv_a_<field>`), голого `x` там нет. Настоящий источник — рукописный `std/src/collections/vec_iter/core.nv` (generic `@min`/`@max`, `export fn[I Next[T], T Compare] I mut @min() -> Option[T]`) и зеркальный `vec_lazy/core.nv`: match-арм `Some(x) => { if x.compare(best) < 0 { best = x } }` биндил голый `x`. `types/mod.rs`'s `match_arm_bindings` (~10630, вызывается из `f1_expr`'s `ExprKind::Match`, ~9072-9113) не резолвит `scrut_ty` для generic `Option[T]` scrutinee `@next()` ВНУТРИ этого же generic-тела (T абстрактен на этапе проверки тела) → `x` не попадает в `scope`. `f1_check_call`'s `ExprKind::Member` dispatch (реальное расположение ~11510-11574 — нумерация маркера ~10553-10617 успела устареть) на пути `obj.method(...)` падает через guard `scope.contains_key(prefix)` на `imported_modules.contains(prefix)` (набор — CU-wide, не per-file) → любой USER-импорт с последним сегментом `x` делает `"x"` известным модулем, и `x.compare(best)` читается как вызов несуществующей свободной функции `compare` в модуле `x` → ложный E7401. Репро подтверждён эмпирически ДО фикса (`nova build` на пакете с `import a.neg.x.{who}` → `[E7401] no function 'compare' in module 'x'` РОВНО на `vec_iter/core.nv:654`/`:669`). Фикс — гигиена НА РЕАЛЬНОМ месте коллизии: `x` → `cand` в обеих функциях (`@min`/`@max`) обоих файлов (`vec_iter/core.nv`, `vec_lazy/core.nv`) + doc-комментарий со ссылкой на маркер; **`auto_derive.rs` и `types/mod.rs` НЕ тронуты** (общий scope-gap для generic match-арм-биндингов — латентная, более глубокая проблема, НЕ фиксится этой волной, т.к. потребовала бы правки checker'а в защищённой зоне; при повторном проявлении — новый маркер). Регресс-фикстура: `nova-cli/tests/ident_x_module_alias_collision.rs` (2 теста — repro `import a.neg.x.{who}` + control `neg.helper`, реальный `nova` бинарь, прецедент `lint_deny.rs`) — 2/2 PASS. Гейты: derive-фикстуры standalone-CU (`d230_clone_deep_autoderive`, `d422_generic_container_derive`, `neg/n1_no_impl_no_autoderive_neg`) `PASS:3 FAIL:0`; `std/src/checksums` + `std/src/collections` (включая сами исправленные `vec_iter/core`, `vec_lazy/core`) `PASS:16 FAIL:0 SKIP:9`; `examples/flagship/aggregator` `nova build` зелёный (`built: aggregator.exe`). Детали — [docs/plans/wip/ident-x-notes.md](wip/ident-x-notes.md). Зона правки: только `std/src/collections/{vec_iter,vec_lazy}/core.nv` + новый тест-файл. **Follow-up (2026-07-21, ветка `p-fix-b6-tail`, sonnet, маркер `generic-match-scope-gap`): общий scope-gap ЗАКРЫТ.** `TypeCheckCtx` теперь публикует enclosing-fn generics+bounds (`current_fn_generics`, RAII-guard, зеркалит `current_fn_return_ty`); `ExprKind::Match` пробует узкий, ADDITIVE fallback (`resolve_generic_bound_method_return`) ТОЛЬКО для feed `match_arm_bindings`, когда scrutinee — 0-arg метод-вызов на generic-параметре, bound к протоколу с этим методом (`Next[T]`) — `infer_expr_type`'s общий decoupling (~15605, намеренный) НЕ тронут. Фикстура `spec_tests/conformance/generic_match_scope_gap.nv` (зеркалит `@min`-форму с ГОЛЫМ bind-именем `x`, RED без фикса → E7401, GREEN с фиксом). | floating | **✅ РЕШЕНО** |
| `[M-d289-module-qualified-path-method-collision-cu]` | **OPEN 2026-07-20 (найдено приёмкой П19/D431 в folder-CU).** 3-сегментный module-qualified вызов `raw_mem.RawMem.copy(...)` (D289-форма `модуль.Тип.метод`) в folder-CU `spec_tests.conformance` МИСДИСПАТЧИЛСЯ на чужой одноимённый метод ДРУГОГО типа из СОСЕДНЕГО файла CU (`Nova_D35Vec2_method_copy`), ресивер эмитнулся буквальным `raw_mem->RawMem` → CC-FAIL `undeclared identifier raw_mem`. 2-сегментная форма (`RawMem.copy` при селективном `import .{RawMem}`) — корректна (класс d231/d299). Родня per-name/CU-wide классов ([M-callnorm-free-fn-name-collision], parfor-capture, ident-x): диспатч по ИМЕНИ метода без учёта module-quality ресивера. Репро: вернуть d431-фикстуре 3-сегментную форму → соло-прогон CC-FAIL. Обход-носитель: d431 переведён на 2-сегментную форму (d4af18030). Зона: checker/codegen D289 Path-dispatch (types/mod.rs Member/Path-армы × emit_c method-резолв). | D289 path-dispatch | **P2** |
| `[M-boehm-large-buffer-retention-fiber-reuse]` | **✅ РЕШЕНО 2026-07-21 (worktree `nova-netb`, ветка `p-fix-net-free-on-close`, sonnet).** Root cause (opus-разведка, `docs/plans/wip/boehmret-design.md`): НЕ conservative-scan false-positive (fiber-стек-гипотеза ОПРОВЕРГНУТА дискриминирующим экспериментом) — реальная утечка в `compiler-codegen/nova_rt/net.c`: `NovaNet2Stream`/`Listener`/`Udp` аллоцируются `nova_alloc_uncollectable` (постоянный GC-рут) и НИКОГДА не освобождались. Вариант (а) (уже влит): обнуление `read_ptr`/`write_req` после op — убрал buffer-scaling утечку (slope 86740→~4820 байт/итер). Вариант (b) (эта волна): free-on-close с counter-based lifetime-протоколом (mn-coding-conventions §9) — classic intrusive atomic refcount (идиом `nova_chan_writer_close`/R1 A1, `nova_aint_fetch_sub_release` + acquire-fence-перед-destroy): `refcount` старт=1 (existence-юнит, отпускается close_cb), каждая op (`accept`/`connect`/`read`/`write`/`udp_send_to`/`udp_recv_from`) держит СВОЙ acquire на всю длительность вызова (goto-based single-exit, покрывает park + послепарковые чтения полей — защита от конкурентного close() из другого файбера); free — у стороны, чей release довёл счётчик до 0. `net_tcp_shutdown` cross-thread (fire-and-forget, без park) — acquire перед очередью/release внутри job. Гейты: slope-репро свой Windows-бинарь 3+3 — ДО (только вариант а) 5249/4803/5264 → ПОСЛЕ (вариант b) 627/671/708 байт/итер (~87% снижение); std/net full 3/3 PASS; nova-tls (WSL) `cert_modes_test` CU 29-30 тестов PASS; stream_leak-смоук (throwaway 150-итер копия, файл-мастер не тронут) 3/3 PASS, slope плато; флагман live (build+run+curl `/`, `/api/snapshot`, `/api/run`×3) все 200, чистое завершение; net.c компилируется без ошибок msvc+clang. Побочная находка: P217 (`emit_c.rs::is_struct_type`, см. новый маркер ниже) ломает std/net+флагман на ЛЮБОЙ платформе на текущем main HEAD — вне зоны этой волны, не чинилось (использован временный reverted scratch-патч только для верификации). Детали/числа — `docs/plans/wip/net-b-notes.md`. Зона правки: только `compiler-codegen/nova_rt/net.c` (net.h не тронут — 0 сигнатурных изменений). | Plan (floating, GC/рантайм) | **✅ РЕШЕНО** |
| `[M-217-is-struct-type-missing-novavalue-prefix]` | ✅ **ЗАКРЫТ 2026-07-21 интегратором ДО этого мёржа — та же регрессия найдена независимо 216-tails-волной и починена в main `45f047098` (точечно в хойст-сайте `enter_defer_scope` ~25940: `NovaValue_`/mono-`____` → `{0}`; сам `is_struct_type` сознательно не расширялся перед релизом — узкий фикс безопаснее; при следующем заходе в emit_c можно генерализовать). Флагман + folder-CU 517/0 зелёные с фиксом.** Исходная запись: OPEN 2026-07-21 (найдено попутно при верификации net-b `[M-boehm-large-buffer-retention-fiber-reuse]` варианта (b), worktree `nova-netb`).** Plan 217 (auto-cleanup, merge `22f3a519f`, уже в main HEAD) ломает компиляцию ЛЮБОГО `.nv`-файла с паттерном `Ok(consume x) => {...}` над struct-типизированным Nova-значением (`TcpStream` и вся `std/net`-семья) — на **любой платформе/тулчейне** (msvc, clang; воспроизведено байт-в-байт на пристинном main-бинаре ДО этой волны). Корень: `compiler-codegen/src/codegen/emit_c.rs:41934` `is_struct_type()` классифицирует C-тип по префиксу (`Nova_`, `NovaVtable_`, `NovaOpt_`, `_NovaTuple`, `struct `), но **пропускает `NovaValue_`** (напр. `NovaValue_TcpStream`) — Plan 217's hoisted-cleanup-пролог (`enter_defer_scope`, ~25922) поэтому эмитит `NovaValue_TcpStream s = 0;` (int-литерал) вместо `NovaValue_TcpStream s = {0};` для STRUCT-типа → `C2440`/`incompatible type 'int'` на КАЖДОМ таком тесте. Блокирует: весь `std/src/net/*_test.nv` (все 13 файлов, любая ОС) + `examples/flagship/aggregator` (4 сайта). Однострочный фикс — добавить `\|\| ty.starts_with("NovaValue_")` в `is_struct_type` (проверен как scratch-патч этой волной, работает, НЕ закоммичен — вне зоны net-b). Зона: `compiler-codegen/src/codegen/emit_c.rs` (checker/codegen, не мой домен). | floating (codegen/emit_c) | **P1 (широкий блокер — весь std/net + флагман)** |

## P2/P3 — найдено при чистке docs/simplifications.md (2026-07-18, хроники архивированы в docs/history/simplifications-closed.md)

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-parfor-tuple-corpus-scale-order-sensitive]` | **OPEN (найдено 2026-07-13 при закрытии `[M-parfor-record-result-miscompile]`, Plan 173.1, ветка `parfor-173-1`).** На масштабе ВСЕГО `spec_tests/conformance` (~950 тестов, один CU) анонимный-tuple под-тест (`(i, i*i)` из Range) детерминированно (100%, воспроизведено и на baseline-бинаре) даёт неверную сумму ПРИ ВЫБОРЕ entry-файла с одним конкретным соседним `.nv` (`c_keyword_ident_mangling.nv`/директория), но ПРОХОДИТ при выборе другого entry-файла того же папки-модуля (folder=один модуль, ожидался byte-identical результат независимо от entry — не подтвердилось: разный `t-<hash>` build-id). Anon-tuple parallel-for уже покрыт `nova_tests/err173_1/parfor_elem_matrix.nv` на меньшем масштабе (стабильно PASS) — исключён из conformance-файла, чтобы не блокировать гейт. Кандидат: tuple-mono-instance naming/counter collision при большом числе зарегистрированных generic-инстансов. Требует отдельной state-dump-style инвестигации. | floating (codegen, tuple-mono) | P2 |
| `[M-tls-xpkg-trybang-value-ptr-dispatch]` | **OPEN (найдено 2026-07-15 при закрытии `[M-tls-xpkg-tlsversion-value-ptr-dispatch]`, ветка `fix-tlsversion-dispatch`).** Соседняя `Try`/`Bang`-ветка (`compiler-codegen/src/codegen/emit_c.rs::infer_expr_c_type`, ~54152) несёт тот же нераскрытый `_p`-маркер (payload-идентификатор не разворачивается через `desanitize_c_from_ident`) для cross-package `Option[Sum]?`/`!!` — другой символ/путь, чем уже закрытая `??`(Coalesce)-ветка (~54063). НЕ воспроизведено предметно (echo_client использует `??`, не `?`), оставлено наблюдением при закрытии соседнего маркера. Нужен репро cross-package sum-type + `?`/`!!` для подтверждения и зеркального фикса. | floating (codegen, emit_c.rs) | P3 |
| `[M-http-decompress-checksum-type-gap]` | **OPEN (найдено 2026-07-10 волной std-hygiene, ветка `std-hygiene`).** http decompress-путь тесты (`mock_roundtrip`/`decompress`/`body_test`) — CODEGEN-FAIL `[E_UNKNOWN_TYPE] Checksum` на `compress/error.nv:121`; pre-existing, не регрессия (есть на main без диффа этой волны), вне скоупа std-hygiene. Требует отдельного расследования (тип `Checksum` не резолвится в этой позиции — возможно связано с `crc32`/`adler32` дедупом `[M-compress-checksum-cleanup]`, но не подтверждено). | floating (compress/http) | P3 |

## P1 — Correctness / Safety / Debuggability

| Маркер | Суть | Home | Pri |
| `[M-mut-binding-accepts-must-consume]` | **✅ РЕШЕНО 2026-07-21 (worktree `nova-mutcons`, ветка `p-fix-mut-consume-bind`, sonnet; заведён по факту и закрыт той же волной).** Находка владельца, читая `examples/tls/echo_server.nv:42-45`: `mut lst = match TcpListener.bind(addr) { Ok(consume l) => l, Err(_) => panic(...) }` — match-РЕЗУЛЬТАТ (must-consume payload, честно потреблённый arm'ой через `Ok(consume l)`, D157-амендмент) биндится в `mut` вместо `consume`, и компилятор МОЛЧИТ. Спека-сверка подтвердила: это gap, не легализованная форма — D180 Rule 1 обязан требовать `consume X = expr` для must-consume RHS. Корень — `infer_value_type` (D180 Rule 1's RHS-инференция, `compiler-codegen/src/types/mod.rs`) никогда не рекурсировал в `ExprKind::Match`/`ExprKind::IfLet` arm-тела — только `Call`/`Ident`/`RecordLit`/`Try`/`Bang`/`RefArg`/`Coalesce`. Фикс — recursion в match-arm/if-let-branch tail (best-effort, первая resolve'ящаяся arm). Побочный фолаут той же волны (обнажился, как только match-tail-passthrough биндинги стали реально consume-obligated): `ExprKind::Match`'s state-join был НЕ divergence-aware (в отличие от `consume_walk_if`, у которого уже был `then_diverges`/`else_diverges`) — классический `Err(e) => { x.close(); return Err(e) }` idiom спуриозно читался как `MaybeConsumed` на arm'ах, которые фактически проходят мимо error-ветки; исправлено тем же слиянием (diverging arm исключается из join'а, первая non-diverging arm тоже проходит через `consume_join` — иначе arm-локальные pattern-биндинги текли бы в states-карту после match'а). Аудит `std/**`+`examples/**` (вне `_wip`) нашёл и канонизировал ~17 сайтов (fs.nv File.open/create, std/net 4 test-файла, tls/net echo_server, aggregator aggregate.nv+main.nv); 3 сайта (`UdpSocket`/`TcpListener`, БЕЗ `@cleanup`, в отличие от `TcpStream`/`TlsStream`) обнажили pre-existing тестовый пробел — `panic(..)` как exit-point требовал закрытия ресурса, тесты дозакрыты явным `.close()` перед panic-веткой. Отдельно рассмотрено и НЕ сделано — «убрать rebind + ручной `.close()`» для `Ok(consume session) => { consume stream = session; …; stream.close() }` идиомы (гипотеза «избыточно раз есть D432 auto-`@cleanup`»): эмпирически ОПРОВЕРГНУТО — (1) D432 auto-cleanup буквально ограничен bare `consume X = e;` (`Stmt::Let`), arm-bound `Ok(consume stream) => {...}` без rebind'а НЕ покрывается вообще (проверено на сгенерированном C — cleanup не вызывается); rebind — load-bearing; (2) отдельное пробное репро с `return`-до-конца-функции внутри вложенного match вызвало runtime-fatal (`D188-on-exit-double-invocation`) — auto-cleanup под этой формой ветвления не полностью надёжен (`[M-d432-early-return-nested-match-disarm]`, НЕ заведён отдельно — echo-файлы этот `return`-паттерн не используют, риска нет). Гейты: репро RED(молчит)→GREEN(`E_CONSUME_KEYWORD_MISSING`) на минимальной фикстуре; все 17 канонизированных сайтов GREEN; 5/5 flagship-целей (`aggregator`, `echo_{server,client}_net`, `echo_{server,client}_tls`) собраны `nova build --strict-effects` (release) чисто; net+tls echo-пары прогнаны END-TO-END (реальный accept/echo/close, не только компиляция); 9 представительных d133/d157/d180 фикстур (изолированные module-копии вне мега-CU, мега-CU НЕ гонялся по инструкции) — 0 регрессий, каждый neg держит СВОЙ ожидаемый код, каждый pos зелёный; новые `spec_tests/conformance/neg/d180_match_tail_mut_binding_neg.nv` (RED-пин) + `spec_tests/conformance/d180_match_tail_consume_binding_ok.nv` (GREEN-пин, оба test-блока реально исполнены test-build'ом, не только type-check). D-амендмент — [D180](../../spec/decisions/05-memory.md#d180-consume-binding-syntax-plan-731) («Амендмент (2026-07-21) — match-tail passthrough как Rule 1 RHS; divergence-aware match join»). | D180/D131, checker consume-flow (`compiler-codegen/src/types/mod.rs`) | **✅ РЕШЕНО** |
| `[M-d180-consume-propagation-match-payload-mut-rebind]` | **В РАБОТЕ — home = [План 216](216-consume-enforce-a.md) (вариант А, волна запущена 2026-07-18).** Было: OPEN 2026-07-17 (нашёл владелец, читая TLS-регресс-тест; подтверждено интегратором по спеке+коду).** Цепочка `match rvalue { Ok(x) => ... } → mut y = x → y.consume_method()` ОБХОДИТ статическую consume-дисциплину. Три сцепленных пробела: **(а)** owning-семантика биндингов пейлоада при rvalue-скрутини нигде не специфицирована — D157 расписывает только place-match (view-default, `Some(consume f)` для move); **(б)** чекер НЕ enforc'ит D156-пропагацию consume-обязательства через Result/Option-пейлоад в match/if-let: pattern-bound значение consume-типа не помечается consume-obligated → вся D180-дисциплина ниже по потоку молчит (симптом-родня: `[M-tls-tests-consume-keyword-d180-drift]` — W_CONSUME_KEYWORD_UNNECESSARY там, где по букве consume обязателен); **(в)** `mut stream = session` (alias/move consume-значения) компилируется вопреки D180 Rule 2 (`E_VIEW_BINDING_FORBIDDEN`); легальная форма — `consume stream = session`: consume = owned-ось D184-триады и УЖЕ несёт права мутации (пример самой спеки D180: `consume sb = StringBuilder.new(); sb.append(...)` при `fn StringBuilder mut @append`), Rule 4 отвергает `consume mut` как ИЗБЫТОЧНОСТЬ, не как пробел (поправка владельца 2026-07-17 к первой редакции этого маркера — «буква не даёт выразить» было неверно; носители: examples/tls/echo_client.nv:44-48, aggregator live.nv:158-163, tls-тесты). Исполнение корректно (фактический move, память под GC), но логическая линейность течёт: double-close/use-after-close на этом пути статически НЕ ловятся. **Нужно дизайн-решение владельца** (язык-меняющее → D-амендмент в том же слиянии): вариант А — enforce по букве: D157-амендмент для rvalue-скрутини (consume-паттерны `Ok(consume tcp)`), enforce D156-пропагации в чекере, миграция сайтов механическая (`mut X = consume_var` → `consume X = consume_var`; амендмент Rule 4 НЕ нужен — consume уже mut-capable); вариант Б — узаконить неявный move для rvalue-пейлоад-биндингов и `mut X = consume_var` (Rust-стиль implicit move, D157/D180-амендменты). Рекомендация интегратора: А (философия D180 «visible ownership transfer на каждом binding-site» — прямо заявленный Nova edge; и ровно этого ожидал владелец интуитивно). До решения — НЕ стоп-волна (существующий код под гейтом опирается на текущее поведение). | D180/D157/D156, checker consume-flow | **P1 (design-blocked)** |
| `[M-i64-clamp-primitive-collision-dispatch]` (= `[M-primitive-concrete-overload-receiver-dispatch]`, дубликат-имя из `9b02925d3`, поглощено при мёрже `64f3369fa`) | **✅ РЕШЕНО 2026-07-16 (Plan [196.9](196.9-primitive-concrete-overload.md), ветка `p196-9-overload`, worktree `nova-p196-9`).** Root cause был НЕ в codegen-диспетче и НЕ в "чекер коэрсит i64→int" (обе исходные гипотезы неточны) — а в `types/mod.rs::f1_expr_inner`'s `ExprKind::Match`: каждый арм обходился с ТЕМ ЖЕ `scope`, что и наружный match, БЕЗ добавления pattern-bound имён (`Some(r) => …` не клало `r` в `scope`) → `check_instance_overload`'s `infer_arg_ty(obj, scope)` не находило `r` → функция молча возвращалась: ни `[E_UNKNOWN_METHOD]`, ни запись в `resolved_callees` — для ЛЮБОГО метод-вызова на pattern-bound ресивере. Фикс: перед обходом тела арма `scope` расширяется биндингами `match_arm_bindings(arm.pattern, scrut_ty)` (Plan 172.1 АТОМ 2a, уже существовал, просто не был подключён к этому обходу), восстанавливается после (save/restore, тот же паттерн, что уже используется для closure-arg seeding). Один фикс закрывает ОБА слоя: (а) `resolved_callees`-канал (Plan 196.7, уже в дереве) теперь видит receiver → диспетчит по ТИПУ для валидных оверлоадов (int/f64 `@clamp` продолжают дispatch'иться раздельно, подтверждено сгенерированным C: `Nova_int_method_clamp`/`Nova_f64_method_clamp`, без cross-cast); (б) primitive-`[E_UNKNOWN_METHOD]`-гейт (Plan 177 Ф.3, уже в дереве) теперь видит `i64` без оверлоада → честная диагностика вместо тихого мис-диспатча в `f64`. `i64 @clamp` НЕ добавлен в std (API-решение владельца, см. 196.9 Follow-up §2) — `duration.nv::sat_add_i64`/`sat_sub_i64` (`nova-p200dur`, Пункт 10/200, `duration.nv:380,387`) теперь падают компиляторной `[E_UNKNOWN_METHOD]` ВМЕСТО тихого RUN-FAIL — тalli `std/src/time`: было `PASS:4 FAIL:2` (маскированный RUN-FAIL), стало `PASS:1 FAIL:5` (честный CODEGEN-FAIL, каскадит на зависимые модули) — числовая талли-регрессия, НЕ regression по корректности; Пункт 10/200 остаётся заблокированным до решения владельца по семантике `i64.clamp`. `ExprKind::IfLet` имеет ТОТ ЖЕ класс scope-пробела — НЕ тронут (нет живого репро, см. 196.9 Follow-up §1). Было ранее: **OPEN 2026-07-16 (найдено попутно при верификации Plan 196.8, вне его объёма — НЕ бланкет-коллизия).** `std/src/runtime/defaults.nv` объявляет `@clamp` ТОЛЬКО для `int` и `f64` (нет `i64`-оверлоада, нет бланкета). `duration.nv::sat_add_i64` (Пункт 10/200) зовёт `r.clamp(lo, hi)`, где `r` — i64, PATTERN-BOUND из `match a.checked_add(b) { Some(r) => ... }` (`Option[T]`-деструктуризация бланкета, T=i64). Чекер **НЕ флагает** `E_UNKNOWN_METHOD` в этой pattern-bound форме (та же «receiver-inference gap», что 196.7 отметил для codegen — здесь, похоже, аналогичный пробел и на стороне ЧЕКЕРА: голая `ro r i64 = …; r.clamp(...)` вне pattern-контекста ДАЁТ `E_UNKNOWN_METHOD` корректно, изолированный репро подтверждён). Codegen молча мис-диспатчит primitive-ресивер БЕЗ подходящего конкретного оверлоада и БЕЗ бланкета в name-keyed `method_receivers` last-wins → попадает в `Nova_f64_method_clamp(int64_t, int64_t, int64_t)` (implicit int64_t↔double C-conversion на границе вызова — НЕ CC-FAIL, т.к. оба скалярны; f64-мантисса 53 бита теряет точность на i64-крайних значениях) → **RUN-FAIL, не CC-FAIL** (тихо неверное значение). Репро: `spec_tests/conformance` изолированный файл (не коммичен — красный тест не оставляем в дереве; описание воспроизводит `sat_add_i64(-1, -i64.MAX, -i64.MAX, i64.MAX)` даёт НЕ `-i64.MAX`). Симптом наблюдался как `nova test std/src/time` → `duration` RUN-FAIL `Ф.1c/D317: Duration saturating_* clamps to ±MAX (no wrap)` — duration.nv:1276 — ПОСЛЕ фикса 196.8 (маскировался предыдущим CC-FAIL, теперь непосредственно виден). Оркестратор-подтверждение по сгенерированному C (2026-07-16): `duration.c:8580/8604/8628` — все три sat_-сайта зовут `Nova_f64_method_clamp(r, lo, hi)` с i64-аргументами. Семейство D164/196.7/196.8 (name-keyed last-wins), но НОВАЯ форма: concrete-vs-concrete (int/f64), ни бланкета, ни точного i64-оверлоада — плюс чекер-side пробел на pattern-bound receiver. **Блокирует Пункт 10/200** (тот же owner-контекст, что закрытый 196.8-маркер). **АМЕНДМЕНТ (волна «числовой паритет», 2026-07-19, worktree `nova-numparity`):** «`i64 @clamp` НЕ добавлен в std» — устарело. Отдельным решением **Plan 200 Step 0 (D74 amend, владелец 2026-07-16)**, ПОСЛЕ записи выше, добавлен `fn[T Ints] T @clamp(lo T, hi T) -> T` (std/src/prelude/protocols.nv) — покрывает i8/i16/i32/i64/int/u8/u16/u32/u64/uint одним бланкетом; `duration.nv::sat_add_i64`/`sat_sub_i64` используют его без изменений. Полный `nova test std/src/time` прогнан этой волной: **PASS:6 FAIL:0 SKIP:1** — Пункт 10/200 закрыт фактом прогона (docs/plans/200-std-improvements.md §Пункт 10, доп. запись). Маркер полностью разрешён, блокера больше нет. | Plan 196 (одно окно) / codegen + checker | **P1** ЗАКРЫТО |
| `[M-208-vec-chained-debug-display-red]` | **✅ РЕШЕНО 2026-07-20 (worktree `nova-vecdbg`, ветка `p-fix-208-vec-chained-debug`, sonnet).** Root cause — НЕ формат (Vec-Display byte-identical до/после 208: `git diff` двух родителей мержа `e06bfb7fa` по `std/src/collections/vec/protocols.nv` показывает только сигнатуру `(mut w Write)`→`(mut f Fmt)`, тело `f.write("Vec[")...f.write("]")` не менялось) — а calling convention: D422 (Plan 208 Ф.2) сделал `Display.@display`/`Debug.@debug` обязательными с параметром `Fmt` (строго богаче `Write`: `use Write` + width/precision/align/fill/sign/alternate/kind/`@pad`), `StringBuilder` реализует только `Write` → голый `.debug(sb)`/`.display(sb)` больше НЕ матчит `Fmt`-параметр напрямую, нужна явная `FmtCtx.bare(sink, mark, is_debug)`. Это УЖЕ документированное и применённое той же волной решение (НЕ моя гипотеза) — `spec_tests/conformance/d374_write_sink_decouple.nv` прямым текстом объясняет тот же переход, `std/src/collections/vec/protocols_test.nv` и `std/src/time/duration/core.nv` используют идентичный `FmtCtx.bare(...)` паттерн; `vec_f32_chained_debug.nv` — единственный НЕ мигрированный fixture (написан до 208 под старую `Write`-подпись), пропущен 208 Ф.3 grep-аудитом (scoped на `std/**`-импленты, не на call-сайты в `spec_tests/`). Фикс: 5 вызовов `.debug(a)`/`.display(a)` → `.debug(FmtCtx.bare(a, 0, true))` / `.display(FmtCtx.bare(a, 0, false))`; assert-строки (`"Vec[...]"`) НЕ менялись — не ослабление, синхронизация с уже установленным каноном D422. Побочная находка (задокументирована, НЕ фикшу здесь — отдельный риск вне объёма): `resolved_cat_of_depth` (`compiler-codegen/src/types/mod.rs`) мапит ЛЮБОЙ `TypeDeclKind::Protocol` expected-тип → `ResolvedType::Any`, из-за чего `assignable_direct` пропускал ЛЮБОЙ аргумент для protocol-typed параметра без структурной проверки — отсюда ДО фикса `.debug(a)` с `a: StringBuilder` компилировался БЕЗ ошибки и давал type confusion на C-уровне (`Nova_StringBuilder*` передавался туда, где ожидался `Nova_FmtCtx*`, оба типа начинаются с указательного поля на смещении 0 → тихая порча вместо крэша, итог — пустая строка вместо `"Vec[...]"`) вместо чистой ошибки компиляции — общий (не Vec-специфичный) пробел чекера, широкий и рискованный фикс, вне объёма этого маркера. Гейты: `spec_tests/conformance` standalone single-CU **PASS 504/FAIL 0/SKIP 14** (было 503/1/14); `std/src/collections` **PASS 13/FAIL 0/SKIP 6** (без регресса); `examples/flagship/aggregator --strict-effects` собран чисто. Детали — [docs/plans/wip/208-vec-debug-notes.md](wip/208-vec-debug-notes.md). Родня `[M-208-generic-interp-display-dispatch-gap]` (208-impl-progress §848, уже ✅ РЕШЕНО отдельно). | 208-волна (Fmt) | **✅ РЕШЕНО** |
| `[M-checker-protocol-typed-arg-any-bypass]` | **✅ РЕШЕНО 2026-07-20 (worktree `nova-protoany`, ветка `p-fix-protocol-any-bypass`, sonnet).** Root cause подтверждён: `resolved_cat_of_depth` (`compiler-codegen/src/types/mod.rs`) мапит ЛЮБОЙ `TypeDeclKind::Protocol` expected-тип → `ResolvedType::Any` (не тронуто — слишком широкий blast radius для других consumers), и `assignable_direct` на самой первой ветке (`if matches!(exp_rt, ResolvedType::Any) { return Compat::Ok }`) пропускал ЛЮБОЙ аргумент для protocol-typed параметра без структурной проверки. Фикс — ХИРУРГИЧЕСКИЙ, только в `assignable_direct`: перед возвратом `Ok` на Any-ветке зовёт новый `self.protocol_mismatch_found(expr, expected, exp_gs, scope)` — детектит, действительно ли `expected` (peeled через readonly/mut/uninit/ref) — Named-ссылка на `TypeDeclKind::Protocol` ИЛИ inline `TypeRef::Protocol{..}` (D142 anon), и если аргумент резолвится в конкретный non-generic non-primitive Named/Array тип — СТРУКТУРНО проверяет через новые `protocol_required_missing`/`protocol_missing_methods` (метод по имени+арности через `method_overloads` = `sig.method_table` ∪ synth/auto-derive overlay, U.2.3.3; `default_body` D183-фолбэк; `use`-embed (D145) развёрнут рекурсивно, зеркалит `BoundCtx`'s `flatten_dfs`). Несоответствие → `Compat::Bad{found}` — переиспользует СУЩЕСТВУЮЩУЮ diagnostic-инфраструктуру каждого call-site (`[E7301]`/`[E_NO_MATCHING_OVERLOAD]`), новый код ошибки НЕ заводился. Отдельная защита от false-positive: если "конкретный" тип аргумента САМ является `TypeDeclKind::Protocol` (D142 protocol-литерал `protocol Name {...}` — методы захвачены в vtable на месте конструкции, НЕ зарегистрированы в `method_table` под именем протокола — ловилось на `pos_protocol_lit_three_caps.nv`), проверка пропускается (permissive, как раньше). `BoundCtx`/generic-bound-путь (`[T Bound]`, D53/D72/D142) НЕ тронут — отдельная структура/фаза, этим багом никогда не страдал. Гейты: `spec_tests/conformance` standalone single-CU **508 PASS / 0 FAIL / 14 SKIP** (было 504/0/14 baseline + 4 новых neg-фикстуры, все PASS); byte-parity для всех ПРОШЕДШИХ фикстур гарантирована КОНСТРУКТИВНО (фикс живёт только в checker, `Compat`-решения не читаются codegen — для файла без нового `Compat::Bad` codegen-путь не меняется вообще). NEG-матрица (4 новых, `spec_tests/conformance/neg/`): `neg_protocol_param_missing_method` (простой missing-метод у NAMED protocol-параметра), `neg_protocol_param_embed_incomplete` (реплика реального прецедента StringBuilder/Fmt: `use`-embedded метод не реализован), `neg_protocol_param_anon_missing` (D142 inline anon-protocol как ПРЯМОЙ параметр, не generic-bound), `neg_protocol_param_wrong_arity` (то же имя метода, другая арность) — все `[E7301]`/E_NO_MATCHING_OVERLOAD "does not satisfy". D-амендмент — [D53](../../spec/decisions/02-types.md#d53-унификация-protocol-под-type-protocol-как-kind-токен) (AMEND 2026-07-20): «PLAIN protocol-typed параметр требует структурного соответствия» — was permissive-only, now enforced. **Побочная находка (НЕ фикшена здесь, вне объёма — залогирована отдельным маркером ниже):** `[M-protocol-box-callarg-vtable-incomplete]` — codegen-гэп в vtable/box-коэрсии для protocol-typed CALL-ARGUMENT позиции, ПРЕДСУЩЕСТВУЮЩИЙ (подтверждён на неисправленном `main`-бинаре, НЕ регрессия этой волны). | checker/звучность | **✅ РЕШЕНО** |
| `[M-protocol-box-callarg-vtable-incomplete]` | **✅ РЕШЕНО 2026-07-21 (Plan 221 A-B4 «box-vtable P2» + A-B6 «latent protocol-box» — ОДИН корень, оба пункта плана закрыты этим фиксом; worktree `nova-boxvt`, ветка `p-fix-box-vtable`, sonnet).** Root — ДВА сцепленных пробела в `compiler-codegen/src/codegen/emit_c.rs`, оба вокруг протокол-типизированной ПАРАМЕТР/ВОЗВРАТ позиции: **(1)** `fn_protocol_params` (таблица «у этого callee параметр №i — протокол, боксуй аргумент») регистрировался ТОЛЬКО внутри `emit_fn` для КАЖДОГО callee, без пре-пасса — caller, эмитящийся РАНЬШЕ своего callee (порядок объявления/эмиссии в исходнике), видел пустую запись в момент эмиссии call-сайта и пропускал бокс ДАЖЕ для generic-протокола (`Greeter[int]`). **(2)** `protocol_type_args` (`emit_c.rs:9183`, до фикса) безусловно возвращал `None`, когда `generics.is_empty()` — т.е. ЛЮБОЙ NAMED non-generic протокол (`type Greeter protocol { @greet()->int }`, без `[T]`) был невидим для ОБОИХ хуков (call-arg pre-box через `fn_protocol_params` И return-value pre-box через `protocol_box_return_type_info`/`current_fn_returns_protocol`) — при этом `type_ref_to_c` (лоуэринг ДЕКЛАРИРОВАННОГО типа сигнатуры, `emit_c.rs:4301-4308`) уже безусловно (без гейта на generics) лоуэрил такой параметр/возврат в `NovaBox_<Proto>` — сигнатура callee требовала box, а call/return-сайты продолжали передавать/возвращать голый `Nova_X*` → `clang: error: passing/returning 'Nova_X *' … incompatible type 'NovaBox_<Proto>'`. Оба репро из исходной записи ниже — ОДИН и тот же корень (2), не два разных: NAMED protocol без embed воспроизводит (2) напрямую; `use`-embed репро из исходной записи ТОЖЕ пробивало (2) первым (тот же CC-FAIL на несовпадающем типе), но за ним обнаружился ВТОРОЙ, независимый и НЕ исправленный этой волной баг — см. новый маркер `[M-protocol-embed-vtable-missing-method]` ниже. Фикс: (а) пре-пасс над `module.items`+`peer_files`, эмитируемый ДО основного цикла (зеркалит уже существующий пре-пасс для non-generic-protocol-typedef рядом), закрывает (1); (б) `protocol_type_args` для `generics.is_empty()` теперь возвращает `Some((proto, vec![]))` вместо `None`, закрывает (2) — плюс два форматера box-имени (`protocol_box_c_type_for` и call-arg pre-box в `emit_call`), читающие результат, поправлены не добавлять хвостовой `_` при пустых `type_args` (иначе `NovaBox_Greeter_` — несуществующий C-тип, вторичный баг что вскрылся бы сразу после (2)). Фикстура `spec_tests/conformance/m221_protocolbox_callarg_ok.nv` — 3 позитивных теста (non-generic call-arg, non-generic return, generic call-arg с callee, объявленным ПОСЛЕ caller). Гейты: репро (3 варианта) RED→GREEN; `spec_tests/conformance` мега-CU **PASS 130/FAIL 0/SKIP 18** (включая новую фикстуру); флагман `examples/flagship/aggregator` `nova build --strict-effects` чистый, `nova test --strict-effects` 9 PASS/1 SKIP + 1 RUN-FAIL (`aggregate.nv:45`, deadline-fan-out timing assert) подтверждён pre-existing флаки — standalone PASS дважды, причина (M:N-scheduling под нагрузкой) не пересекается с protocol/vtable кодом. Коммит `847cdbc84`. Было ранее — **OPEN 2026-07-20** (найдено попутно при POS-регрессионном тестировании фикса `[M-checker-protocol-typed-arg-any-bypass]`, worktree `nova-protoany`): два «независимых» репро, гипотеза «let-binding работает, call-argument нет» — уточнено этой волной: гипотеза была НЕТОЧНА (let-binding non-generic protocol coercion — `mut g Greeter = sp` — тоже страдала бы тем же (2), если бы её протестировали ИМЕННО на non-generic; `f2_protocol_dispatch_method_survives.nv`, приведённый как «работающий контраст», НЕ non-generic-без-embed случай в чистом виде — переисследовано и переформулировано в этой записи). | codegen / protocol-dispatch | **✅ РЕШЕНО** |
| `[M-protocol-embed-vtable-missing-method]` | **✅ РЕШЕНО 2026-07-21 (worktree `nova-embvt`, ветка `p-fix-embed-vtable`, sonnet).** Гипотеза записи подтверждена. Корень: codegen НЕ зеркалит чекер-side `use`-embed flatten (`types/mod.rs`'s `flatten_dfs`/`protocol_missing_methods` — bag-union по `embeds`, cycle-guard через `seen`, D145) — `protocol_method_registry` (`emit_c.rs`, оба пре-пасса регистрации — non-generic ~строка 5957 и generic ~строка 6112 до фикса) вставлял СЫРОЙ `t.kind`'s `methods` без учёта `embeds`. Vtable STRUCT (`emit_protocol_box_typedef`) и vtable INSTANCE (`emit_protocol_vtable_companion`) строятся из этого реестра — без flatten у embedded-метода нет ни поля в структуре, ни thunk'а в инстансе → `clang: error: no member named 'base_greet' in struct NovaVtable_EmbGreeter`. Фикс: пре-пасс СРАЗУ перед обоими registration-циклами собирает `protocol_direct: HashMap<String,(type_params, methods, embeds)>` из `module.items` + `peer_files` (cross-file embed'ы, зеркалит checker'ов CU-wide lookup), новая рекурсивная `flatten_protocol_methods_codegen` (мираж `types/mod.rs::flatten_dfs` — тот же bag-union + `seen`-cycle-guard) прогоняется ПЕРЕД `.insert(...)` в оба существующих сайта — 0 новых точек регистрации, только замена `methods.clone()` → `flattened_protocol_methods(&t.name, &protocol_direct)`. Побочная находка: `spec_tests/conformance` мега-CU (1005 плоских файлов = ОДИН compile-unit = ОДНА тест-запись) молча SKIP'ался ЦЕЛИКОМ на любом прогоне (в т.ч. на прошлых волнах, включая закрытие маркера выше) из-за ложного срабатывания `test_runner.rs`'s `parse_smt_backend_requirement` на строке-комментарии в `d256_contract_self_field.nv` («// REQUIRES_SMT_BACKEND; здесь — runtime-enforcement...» — доковый текст, НЕ директива, но парсер матчит по голому префиксу без синтаксис-проверки) — peer-file marker-scan (folder-module-wide) кормил эту ложную запись во ВСЮ агрегированную CU-тест-запись → 0 реальных тестов исполнялось молча. Исправлено (переформулирован комментарий, без изменения смысла/семантики) — иначе гейт этой волны (и всех предыдущих, ссылавшихся на «PASS 130/FAIL 0/SKIP 18» для соседнего маркера) был бы основан на никогда не исполнявшемся CU. Фикстура `spec_tests/conformance/m_embvt_protocolbox_embed_callarg_ok.nv` — 2 позитивных теста (call-argument + let-binding коэрсия, обе формы из исходной записи), по образцу `neg/neg_protocol_param_embed_incomplete.nv`'s embed-синтаксиса. Гейты: репро (call-arg + let-binding) RED→GREEN (подтверждено на stash'нутом pre-fix бинаре — `no member named 'base_greet' in 'struct NovaVtable_EmbGreeter'`); `spec_tests/conformance` мега-CU (после SMT-marker-фикса) **PASS 130/FAIL 0/SKIP 18** (включая новую фикстуру + `d374_write_sink_decouple` + `m221_protocolbox_callarg_ok` + `neg_protocol_param_embed_incomplete`); флагман `examples/flagship/aggregator` `nova build --strict-effects` чистый; `std/src/runtime/fmt_buf/core_test` + `std/src/runtime/string_builder_test` — 2 PASS/0 FAIL. | codegen / protocol-dispatch | **✅ РЕШЕНО** |
| `[M-vec-ext-method-untyped-let-breaks-chain-dispatch]` | **✅ РЕШЕНО 2026-07-17 (worktree `nova-untypedlet`, ветка `p-fix-untyped-let-chain`).** Root cause найден в `compiler-codegen/src/types/mod.rs::f3_check_member_ctx` (метод-чек, блок "Метод?"): `ro x = v.map[U](...)` (генерик-метод СО СВОИМ типопараметром `[U]` на `[]T`-ресивере, напр. `vec_seq.nv`'s `@map[U]`/`@filter`/`@fold[Acc]`) без явной аннотации типа биндинга материализует тип `x` через КАНАЛ (`f1_stmt`'s `chain_ty`, читает `resolved_types_buf`) — `ResolvedType::from_type_ref` канонизирует `TypeRef::Array` в `Named{"Vec",[elem]}` (D239), и обратная конвертация (`resolved_to_typeref_tp`) восстанавливает ИМЕННО эту `Named`-форму (НЕ исходный `Array`). Эта форма ДОХОДИТ до метод-чека в `f3_check_member_ctx` (там `TypeRef::Named`-деструктура матчит); а genuine `TypeRef::Array` (от прямой аннотации `ro x []T = ...`) БЕЙЛИТСЯ раньше метод-чека вообще (permissive — чек просто не выполняется), что и маскировало баг для аннотированного случая. Метод-чек знал только 2 из 3 конвенций регистрации slice-методов в `method_table` — bare `"Vec"` (нативные `Vec[T]`-методы) и литеральный `"[]<конкретный-элемент>"` (`fn []str @join(...)`-стиль) — но НЕ литеральный `"[]T"` (СОБСТВЕННЫЙ generic-параметр декларации — ровно `vec_seq.nv`-идиома `fn[T] []T @method[U](...)`). Фикс: третий гейт `prefix_generic_slice_method` рядом с существующим `slice_elem_has_method` — реконструирует `TypeRef::Array` из `recv_type_args[0]` и зовёт уже существующую протестированную `prefix_generic_method_exists` (Plan 177 Ф.3, 0 false-positives/707K вызовов корпуса). Frozen-зона `infer_call_ret_c` (emit_c.rs) не тронута — фикс целиком в checker. Верификация: мини-репро (3 варианта: unannotated/chained-one-expr/annotated-control) RED→GREEN; `nova check nova_tests/generics/mono_basic.nv` (несёт `plan101_1_vec_chained.nv`'s `my_filter_ch`) GREEN (было `[E7320]`) — полный `nova test` на этой folder-module по-прежнему CODEGEN-FAIL, но по ДРУГОЙ, не связанной причине (соседний файл `plan101_1_vec_map_int_str.nv` зовёт ретрактированный Plan-174.2 `str.from(x)` — pre-existing debt, вне объёма этого маркера, НЕ трогал); δ0 GREEN на `std/src/collections/vec_seq.nv` (реальный прод-риск, `@map[U]`+`@filter`/`@fold[Acc]`), `std/src/checksums/{adler32,crc32,fnv}_test.nv`, `std/src/runtime/{char,sync}_test.nv`; новая standalone pin-фикстура `spec_tests/conformance/vec_ext_method_untyped_let_chain_ok.nv` (реальные `vec_seq.map`/`.filter`, unannotated+chained+annotated-control) — `nova check` PASS (полный `nova test` на spec_tests/conformance НЕ гонял — это ОДИН compile-unit, любой файл внутри тянет ВЕСЬ каталог; см. §примечание про `[M-208-vec-chained-debug-display-red]` ниже — не путать: та RUN-FAIL на `vec_f32_chained_debug.nv`/`app_effect_basic_t8_1` — ДРУГОЙ, уже отдельно триажированный P1 маркер, "208-волна", НЕ пересекается с этим фиксом). Было ранее (триаж, 2026-07-17, ветка `p-diag-span-triage`, НЕ смёржена в main на момент этого фикса): бисект на 3 реперных точках показал регрессию НЕ от 196.7/196.8/196.9-волны (уже красно ДО неё), настоящая регрессия компилятора, влетевшая в окне `062bbfa94..c4a075ac6` (~5390 коммитов, точный коммит-виновник не найден — вне объёма триажа). | checker, `types/mod.rs::f3_check_member_ctx` | **✅ РЕШЕНО** |
| `[M-diag-dep-file-span-misattribution]` | **✅ РЕШЕНО 2026-07-20 (worktree `nova-diagspan2`, ветка `p-fix-diag-dep-span`, sonnet).** Root cause — НЕ в file_id-мапе диагностик (`compiler-codegen/src/diag.rs::SourceMap` уже поддерживала per-file_id резолюцию, `Diagnostic::render_with_map`, с Plan 81 Ф.8.1) и не в резолвере импортов (`imports.rs` уже назначает peer-файлам, включая path/git-зависимости, уникальный `file_id` через `resolve_imports_inline_ex`) — а в `nova-cli::cmd_build` (`nova build`, ЕДИНСТВЕННЫЙ путь, которым реально собирается флагман/`nova.toml`-пакеты с deps): type-check error path (`nova_codegen::types::check_module(&module).map_err(...)`) рендерил диагностики через single-file резолвер `d.render(&src, &path_str)`, который ПОЛНОСТЬЮ игнорирует `span.file_id` и применяет byte-offset dep-файла к source+пути ВХОДНОГО файла пакета-потребителя. `cmd_check` (`nova check`) уже нёс этот фикс (`build_source_map`+`render_with_map`, Plan 81 Ф.8.1) — `cmd_build` никогда не получал его (тот же класс `nova build`-lags-behind-`cmd_check`/`test_runner.rs` пробела, что и соседние маркеры Ф.4c в том же файле). Фикс: `cmd_build`'s type-check error path переведён на `build_source_map(&module, &src, &path)` + `render_with_map` (тот же паттерн, что `cmd_check`); заодно (тот же корень, та же функция) — `embed_resolve` error-путь и `lint_module`-warnings в `cmd_build` тоже переведены на file_id-aware резолюцию (были той же однофайловой формы). Репро: `nova.local.toml`-override (`[replace] http = { path = "../../nova-http-diagrepro" }`, detached-HEAD worktree на nova-http@811197c, ДО миграции 250f4ab) — ДО фикса `nova build examples/flagship/aggregator/src/main.nv` выводил 6 ошибок `[E7320] no method into on WriteBuffer` со спанами `main.nv:47/377/111/139/144/159` (комментарии, byte-offset dep-файла применён к main.nv); ПОСЛЕ фикса ТЕ ЖЕ 6 ошибок → `D:\...\nova-http-diagrepro\src\{header.nv:70, url.nv:564, server\wire.nv:162/203/215/243}` — точные реальные строки/сниппеты dep-файлов. Regression: (1) синтетическая ошибка в корневом файле (`domain.nv` + временный `buf.into()`) → `nova build` корректно указал `examples/flagship/aggregator/src/domain/domain.nv:147:5` (без override, corpus untouched после проверки); (2) флагман против РЕАЛЬНОГО текущего nova-http (без override) — `nova build` exit 0, чистая сборка (`built: .../agg_final.exe`), lint-предупреждения из nova-http/std корректно показывают СВОИ пути (`D:\...\nova-http\src\server\wire.nv:108:22` и т.п.), ни одного ложного `main.nv`-спана. Мега-CU не гонялся (не требуется — изменение в nova-cli, не в чекере/resolver'е). Зона: `nova-cli/src/main.rs::cmd_build` (НЕ `types/mod.rs`, НЕ `emit_c.rs` — пересечений не было). | resolver/diag (Plan 202-семья), `nova-cli/src/main.rs::cmd_build` | **✅ РЕШЕНО** |
| `[M-187-sse-live-tls-server-hang]` | **✅ РЕШЕНО 2026-07-15** (фикс `a59800994` в main, conformance 470/0). Корень — НЕ SSE и НЕ M:N-архитектура: `compiler-codegen/nova_rt/runtime.c::nova_runtime_cancel_worker_fibers` безусловно no-op'ила отмену сетевого парка при активном driver-потоке → **любой** `supervised(deadline:)` над живым TCP/TLS/UDP/DNS-парком (`stop_cb`) не отменялся; SSE лишь первым натыкался при повторных запросах. Фикс: driver-режим гейтит только bare-park fallback-ветку; ветка с зарегистрированным `stop_cb` (сеть) выполняется всегда (множества не пересекаются — `_nova_sleep_via_driver` не регистрирует stop_cb). Форс-репро (`LIVE_BUDGET_MS=15`) 100/100 чисто (было 100% зависание на 2-й); официальный гейт 10× SSE-live + 15с idle — сервер жив; demo/chaos/health-live/SSE-demo не регрессировали. Ниже — исходный OPEN-контекст. — **OPEN 2026-07-15 (воспроизведено оркестратором под нагрузкой).** `GET /api/events?legend=weather&mode=live` (SSE-путь + real_net HTTPS/TLS) **вешает сервер НАСМЕРТЬ**: первый запрос отдаёт `replay_info`+частичный `lane_started`, второй → 0 строк, процесс жив но не отвечает (000), нужен kill. Контраст: `GET /api/run?legend=weather&mode=live` (тот же fan-out, БЕЗ SSE) — 5×5 подряд OK, 4/4 done. Значит клинит именно **SSE-стриминг поверх live-TLS-соединений** (не одиночный TLS — тот работает). Гипотеза: SSE-ответ завершается, не дренировав/не отменив remote-park открытых TLS-fiber'ов → застрявший слот (родня `[M-187-watchdog-idle-server-kill]`/#4 pending_remote-семьи, но триггер — SSE+TLS-комбинация). Repro: сервер флагмана (examples/flagship/aggregator, live.nv real-handshake), 2× curl -N /api/events weather-live. Зона: runtime 83.x (remote-park drain на закрытии SSE) + возможно http SSE-хендлер. Демо-обход: браузерный weather-**live** не открывать (demo/chaos/health-live/weather-**demo** стабильны). | Plan 187 / runtime 83.x + http | **✅ РЕШЕНО** |
|---|---|---|---|
| `[M-compress-checksum-cleanup]` | ✅ **CLOSED 2026-07-21 ЦЕЛИКОМ** — nova-compress-синк выполнен (репа nova-compress, ветка `p-checksum-sync` → master `d99c2fe`, sonnet): `checksum.nv` (102 строки дубля) → facade `export import std.checksums.crc32/adler32.{...}`; точечные import в gzip/zlib/checksum_test/d336 (D29 per-file); гейт `nova test src` 1/0 + `--full` 2/0 идентичны базе, 81/81 внутренних PASS. Исходная запись: OPEN 2026-07-15 — std-часть ЗАКРЫТА (ветка `fix-crc32-dedup`, sonnet), nova-compress-синк ОСТАЁТСЯ. (1) **Инлайн-тесты в модуле** — ✅ ЧАСТИЧНО: вынесены из `nova-compress/src/checksum.nv` в `checksum_test.nv` (`47b0a64`, push public); `std/encoding/compress/checksum.nv` тоже несёт 8 инлайн-тестов — но std/compress УДАЛЯЕТСЯ по 205-endgame (чинить при удалении, не отдельно; НЕ трогали). (2) **`crc32` ТРОИТСЯ** → **ДЕДУП через std — (а)+(б) DONE:** `adler32` промоутнут в `std/checksums/adler32.nv` (+ `adler32_test.nv`, RFC 1950 векторы Wikipedia/123456789 + incremental, зеркало `crc32.nv`); `std/encoding/compress/checksum.nv` больше не несёт свою реализацию — `export import std.checksums.{crc32,adler32}` (facade re-export для внешних потребителей модуля). **Важный нюанс D29, подтверждено эмпирически:** peer-file `share namespace` в folder-модуле распространяется на ДЕКЛАРИРОВАННЫЕ имена, но НЕ на имена, которые peer-файл сам импортировал/ре-экспортировал — `export import` в `checksum.nv` НЕ сделал `crc32`/`adler32` видимыми в `gzip.nv`/`zlib.nv`/`d336_checksum_test.nv` без их СОБСТВЕННОГО `import` (per-file, как в Go). Добавлены точечные `import std.checksums.crc32.{...}` в `gzip.nv` и `import std.checksums.adler32.{...}` в `zlib.nv` + `d336_checksum_test.nv`. Гейт: `std/checksums` (crc32+adler32, 4 PASS/3 SKIP-no-test) + `std/encoding/compress` (1 PASS, весь merged-CU включая d336-контракт) — зелено. **(в) nova-compress-синк — ОТДЕЛЬНО** (внешняя репа, `nova-compress/src/checksum.nv` → тот же `import std.checksums.{crc32,adler32}`); НЕ сделан в этой ветке (задание явно исключало трогать внешнюю репу) — push оркестратором ПОСЛЕ вливания std-части. Не correctness (конвенция+DRY); не язык-меняющее (перекладка кода) → D-амендмент не требуется. | Plan 205 / 179 (compress) | P3 |
| `[M-d39-embed-delegation-dispatch-noop]` | **OPEN 2026-07-15 — РАССЛЕДОВАНИЕ ИДЁТ (opus-рекон, worktree `nova-d39`/ветка `recon-d39-embed`).** D39 embed-делегация, call-site диспатч: `Set[T]` встраивает `HashMap[T,()]`; делегированные методы `set.merge_from(b)` = **no-op**, `for v in set.values()` тело **ни разу не исполняется**. Программа компилируется+линкуется, но **делает не то** (тихое неверное поведение). Воспроизводится В ОБЫЧНОМ режиме (single-TU) — **НЕ связан с Plan 209** (209 только вскрыл попутно при верификации Ф.2). НЕ путать с уже ПОЧИНЕННЫМ mono-gap (тела не эмитились под внешней линковкой — исправлено `compute_dead_decls_with`); ТУТ метод вызывается, тело есть, но `emit_embed_proxies` (~emit_c.rs:14881) берёт `base_c_name` без корректной подстановки mono-инстанса embedded-поля / receiver embedded-поля → зовётся не та функция/кривой каст. **Не паркуется** — рекон найдёт корень+красный тест+минимальный фикс, дешёвая модель применит, гейт — интегратор. | floating (fix-волна) | **P1 — FIX IN PROGRESS** |
| `[M-176-io-fs-os]` | Umbrella Plan 176 (io-core/fs/os). **Ф.0.5 + Ф.1 (io-core) — DONE 2026-07-04. Ф.2 (fs+Path) — DONE 2026-07-04:** byte-backed `Path` (POSIX+Windows/UNC/drive, value); `Fs` effect (**thin int-primitive layer** — rich types built in .nv wrappers, sidesteps effect-vtable Err-erasure); `File` must-consume (D133) + OpenOptions(read/write/append/truncate/create/create_new) + positioned read_at/write_at/seek + sync_all/sync_data; `Metadata`(→Timestamp)/`DirEntry`/`FileType`/`Permissions`; `real_fs()` over libuv (`nova_rt/fs.c`, uv_fs_* park/wake, best-effort-cancel) + `mock_fs()`/`MemFs` (in-memory, ENOSPC injection); convenience read/write/read_text/write_atomic(5-step durable)/create_dir_all/remove_dir_all/copy_file/rename/read_dir/canonicalize/symlink/set_permissions; `c_path` NUL-reject (§3c). **Ф.3 (os) — DONE 2026-07-06 (D324):** `std/os` (effect/ffi/os/mock.nv); `Os` effect (thin int/str-primitive) + public `args`/`get_env`(+`_bytes`)/`has_env`/`set_env`(+`_bytes`)/`remove_env`/`vars`(→`[]EnvVar`)/`current_dir`/`set_current_dir`/`temp_dir`/`home_dir`/`exit_process`(flush)/`pid`/`hostname`; byte-first (env `[]u8`, `os_cstr` NUL-reject, non-UTF-8 round-trip via `get_env_bytes`); `real_os()` over native non-blocking hooks `nova_rt/os_env.h` (getenv/`_putenv_s`/setenv/getcwd/chdir/getpid/gethostname; argv captured in `int main(int argc,char**argv)` via `nova_os_set_args`) + `mock_os(MockOs)` (in-memory, recorded-exit `did_exit`/`exit_code`); set_env/set_cwd race-контракт documented. Tests: `nova_tests/os` 6/6 + conformance `d324` (53/0). **Ф.4 (net-миграции) — DONE 2026-07-09 (Q3/Q6):** `NetError.@to_error_kind()->ErrorKind`/`@to_io_error(op)->IoError` — аддитивная best-effort проекция (std/net/error.nv); `NetError`/`@to_str()` строки НЕ тронуты (выбран меньший дифф). `TcpStream.@read`/`@write`(+`@flush()` no-op) мигрированы на `Result[_, IoError]` — структурная io.Read/io.Write conformance поверх byte-surface D407 (уже влитого); остальной `Net`-эффект (write_all/read_to_vec/read_text, halves, UDP/DNS) не тронут. Координация 178: `HttpError.ErrSource.Net(NetError)` разгейчен (`HttpError.from_net`), `std/http/transport/real.nv` использует; опасавшийся namespace-shadow (`NetError.InvalidPort`/`ParseUrlError.InvalidPort`) не подтвердился (переименован в `MalformedPort`). Тесты: `std/net/tcp_test.nv`/`mock_test.nv`/`stress_test.nv` (`must_io` twin) + новый `std/net/d302_neterror_iokind_test.nv`. D302 амендирован (04-effects.md + README). **Ф.5 (docs/Q-sweep) — DONE 2026-07-09:** `docs/io-fs.md` (модель + 7-язык. таблица + differentiators + write_atomic Swift/Zig антипример); `spec/open-questions.md` Q9 частично закрыт (Time/Net/Fs/Os/Io/Http строки → D-ссылки); Q-stdlib-minimal-api `from_bytes` форма уже была обновлена Ф.0.5 (verified). **Гейты финал:** conformance 67/0; std/net (addr/tcp/udp/dns/error) PASS пофайлово; std/io 1/0; std/fs 1/0; std/http 5/0. **Plan 176 ЗАКРЫТ.** | Plan 176 | ✅ DONE |
| `[M-176-consume-through-result-match]` | ✅ **CLOSED 2026-07-24** (окно 67 consume-звучность). Корень: `consume_declare_arm_pattern` регистрирует pattern-обязательство ДО `consume_walk_block` тела arm'а → блочный delta-scoped exit-check видел его как pre-existing outer и пропускал; пост-arm `consume_join` тихо ронял arm-локальный state-ключ, обязательство навсегда оставалось в `consume_obligations` с state=`None`, что `check_obligations_at_exit` трактует как Consumed. Фикс — `check_and_clear_arm_pattern_obligations` (types/mod.rs), exit-check на СВОЁМ arm/then-exit'е, симметрично `Match`/`IfLet`; brace-less tail-passthrough (`Ok(consume l) => l`) дозеркалил bare-Ident mark-consumed исключение. Гейты: consume_through_match_result_ok.nv (pos) + neg/consume_through_match_result_forget_neg.nv; regression 9 d157/d180/detach фикстур + std/net(7)+std/fs(3) targeted + flagship --strict-effects — все GREEN. D-амендмент spec/decisions/05-memory.md (checker soundness, no rule change). | Plan 176 Ф.2 / checker | ✅ done |
| ~~`[M-196-freefn-arity-overload-default-ret-mismatch]`~~ | **✅ ГОТОВ 2026-07-21 (Plan 221.1 Б3, sonnet, ветка `p-fix-freefn-arity`, worktree `nova-freefn`, НЕ влито — интегратор заберёт).** Диагностика уточняет исходную запись: изолированный литеральный репро (`fn f(x int) -> int` + `fn f(x int, tag str = "d") -> str`, `f(5)`) на текущем дереве уже НЕ давал CC-FAIL — попутно нейтрализован НЕсвязанной более поздней волной (`[M-196-rtbuf-producers]`'s Q1-продюсер в `types/mod.rs`'s Ident/Call-арме `f1_expr`, фильтрующий по буквальному `f.params.len() == args.len()` — эта строгая (arity-blind к дефолтам) проверка случайно совпадает с «безопасным» кандидатом). Реальный ЖИВОЙ дефект — та же корневая механика в БОЛЕЕ ШИРОКОЙ форме, где Q1 ТОЖЕ мимо: когда call НИ ОДНОМУ overload'у не соответствует ТОЧНО по общему числу параметров (оба нужны default-fill, разной степени) — тогда чекер (`f1_check_call`) видит genuine ambiguity (`compat.len()>=2`, оба bind_call_args успешны), НЕ пишет `resolved_callees`, а `callnorm.rs`'s `sigs.free` (unconditionally drops any name с >1 сигнатурой) НИКОГДА не бэкфиллит дефолты для такого call'а → падает в CODEGEN-FAIL «no matching overload» (репро: `fn h(x int, y int=10)->int` + `fn h(x int, y int=10, z int=20)->str`, `h(5)` — 1 арг не равен НИ 2 НИ 3 буквально). **Фикс (вариант (b) исходной записи, адаптированный):** новый `TypeCheckCtx::pick_no_default_overload` (types/mod.rs) — когда чекер находит ≥2 arity+type-compatible кандидата, предпочитает того, кому нужно МЕНЬШЕ всего default-заполнений (уникальный минимум; genuine tie — напр. D84 axis-2 same-arity/-type — по-прежнему не резолвится, без изменений). Это ЖЕ автоматически чинит `callnorm.rs`'s default-backfill для этих call'ов (её уже существующий `channel_params` fast-path читает `resolved_callees`). Дополнительно `emit_c.rs` получает параллельный arity-keyed фоллбек (`free_fn_ret_by_arity`, рядом с `user_fn_sigs`, никогда не заменяя его) — для call'ов, чей checker-id потерян `callnorm`'s Block-переписыванием (синтезированный tail-`Call` не несёт id для канала); дизамбигуация по `params.len() == args.len()` корректна, т.к. к этому моменту `callnorm` уже нормализовал call к ТОЧНОЙ арности резолвленного кандидата. **Гейты:** оба репро-шейпа RED→GREEN в изоляции (шейп 1 — литеральный маркер-репро, уже был GREEN, теперь GREEN через ДВА независимых канала; шейп 2 — «оба нужны дефолты» — было CODEGEN-FAIL «no matching overload», стало PASS); регресс запинен `spec_tests/conformance/m196af_freefn_arity_default_ret.nv` (оба шейпа, pos) + `neg/m196af_freefn_arity_default_ret_neg.nv` (genuine wrong-arity call всё ещё падает компиляцией — фикс не глотает честные ошибки); точечный d84+d102 сэмпл (function-overloading + named-args/defaults) прогнан в ИЗОЛИРОВАННОМ мини-CU (не мега-CU) — PASS (d84-файл в дереве временно пойман НЕсвязанной ретракцией `str +`→interpolation, патчен ТОЛЬКО в скретч-копии для гейта, исходный файл не трогался — вне объёма); `examples/flagship/aggregator --strict-effects` собран чисто. Зона правки — ТОЛЬКО `types/mod.rs` (новый метод + 1 сайт) + `emit_c.rs` (новое поле + 2 сайта регистрации/чтения), общие хелперы не рефакторились. Коммит `cbfe56bd4`. | Plan 196 Facet C (callnorm/argbind) / Plan 221.1 Б3 | ✅ ГОТОВ |
| ~~`[M-176-conformance-cu-map-closure]`~~ | **RESOLVED 2026-07-05 (sync-fix-d322).** Root был НЕ «CU-content-dependent closure-env» сам по себе, а **лик глобального `var_mutable` через границы функций**: `emit_fn` НЕ скоупил `var_mutable` (только test-body/spawn-скоупы сохраняли его). `mut f`-локаль в ОДНОЙ функции (появляется когда std.fs-файл добавлен в CU) оставалась в `var_mutable`, и при эмиссии ПОЗЖЕ лямбды `BoxIter[T].map` её ИММУТАБЕЛЬНЫЙ fn-param `f` мис-классифицировался как **by-ref MUT-capture** (env-поле `T** f`, boxed, зарегистрирован в `var_boxed` БЕЗ unpack-локали). Closure-CALL `f(x)` (`NOVA_CLOS_CALL_*`) не консультирует `var_boxed` → голый `f` → undeclared. **Фикс:** скоуп `var_mutable` per-fn-body в `emit_fn` (take at entry → своя `mut`-локали накапливаются → restore at exit; `mut src` map-а всё ещё by-ref корректно). d323-фикстуры возвращены В `spec_tests/conformance` (d102 PASS при них внутри). | Plan 176 Ф.2 / codegen | ✅ DONE |
| ~~`[M-sync-crossmodule-samename-type-collision]`~~ | ✅ **CLOSED 2026-07-06 (D381, ветка fix-nominal-mangling).** Codegen теперь **collision-aware module-qualified** манглит nominal-типы: на входе `emit_module` строится карта `простое-имя → {модули}` (из `peer_files`); имена, объявленные в ≥2 РАЗНЫХ модулях («colliding»), получают квалиф. базу `Nova_<modpath>_<Name>` (+ tag `NOVA_TAG_<base>_<V>`, ctor `nova_make_<base>_<V>`, schema/registry-ключи); **все прочие имена байт-идентичны** (`colliding_type_names` пуст в CU без коллизии → все хелперы no-op). Только plain-Sum + heap-Record (pointer-identity); newtype/value-record/generic/opaque — отдельная ось (followup). DEF-модуль берётся из `t.span.file_id`; REF-резолюция — из файла ссылки (Rule C peer-sharing + selective-import, suffix-match для package-root `std`); bare-variant ctor дизамбигуируется по **арности** payload (`InvalidData(msg)` compress vs io unit) + return-sum контексту (`Other(x)`); registry-fallback по уникальному имени варианта когда file-context недоступен (erased/mono тела). Фикстуры d358/d333-336 **возвращены в `spec_tests/conformance`** — все ТРИ `ErrorKind` (io/http/compress) в одном CU: `nova test spec_tests/conformance` = **PASS 1/0**. Zero-regression: byte-identical content на не-коллидирующем корпусе (io/os/fs/http/serde/compress/plan91_12; расхождения = pre-existing typedef/variant-order nondeterminism [M-codegen-emission-nondeterminism], тот же multiset, baseline тоже флюктуирует). История — simplifications.md. | codegen/resolver | ✅ DONE |
| `[M-effect-handler-body-record-literal]` | ✅ **ЗАКРЫТ 2026-07-22 (175 Ф.2-v2, ветка p175-typed-effects: emit_handler_lit переведён на общий fn-путь как emit_lambda — record literal в handler-теле теперь работает). Исходно OPEN (заведён 2026-07-21 по вопросу владельца; дефект известен с 2026-07-10 — причина 4-кратного отката Plan 175 Ф.2 typed-effect-ops, был задокументирован ТОЛЬКО текстом в 175/D316-amend §Ф.2-находка без маркера).** Codegen тел effect-handler'ов (`with X = effect { op() { ... } }` — особый путь: тело становится C-функцией vtable-слота) БЕДНЕЕ обычного fn-пути: не поддержан anonymous record literal (`Monotonic { nanos: ... }`) — mock-обработчик не может сконструировать value-record → типизированные опы нетестируемы → int-провод (option C). Второй, парный блокер: слои prelude⟷std.time (типы времени не видны прелюдии). Fix-направление (уточнено владельцем 2026-07-21): НЕ дотягивать особый путь — ЗАМЕНИТЬ его: handler-тела опускать в обычные fn общим путём (переиспользуя протокол-машинерию: тела протокол-методов = обычные fn, vtable из указателей — там паритет полный), vtable эффекта собирать из указателей; Time-эффект перенести в std.time (user-эффекты вне прелюдии уже работают — слои НЕ блокер). ОДНО окно. Home = план «175 Ф.2-v2» (221.1 Ф.2б). Родня: `[M-effect-forbid-generic-bound]`. | 175 Ф.2-v2 / codegen handler-тел | P2 |
| `[M-175-time-typed-schema-scalar-bridge]` | **OPEN (2026-07-22, найдено при попытке полной типизации Time-опов в 175 Ф.2-v2).** `sleep(Duration)`/`now()->Timestamp`/`now_monotonic()->Monotonic` (дроп `_ms`/`_ns`-суффиксов) требует wire↔surface scalar-bridge на ДВУХ codegen-точках (handler-impl сигнатура + generic call-site dispatch), т.к. hand-written `NovaVtable_Time` (`nova_rt/effects.h`) компилируется РАНЬШЕ per-CU `NovaValue_Duration`/`Timestamp`/`Monotonic`-typedef'ов — не может именовать их напрямую (та же находка, что D316-amend §Ф.2). Слишком большой/рискованный кусок для окна 175 Ф.2-v2 вместе с capture-mechanism-фиксом + `#default_handler` — отложено отдельным followup. См. [D431](../../spec/decisions/04-effects.md#d431-default_handlerx--ambient-lazy-default-handler-factory-для-эффектов-plan-175-ф2-v2-2026-07-2122) §«Границы». | 175 Ф.2-v3 (следующее окно) | P2 |
| `[M-175-time-ambient-retraction]` | **OPEN (2026-07-22, владелец запросил в ходе 175 Ф.2-v2, не начато).** Ретракция ambient-статуса `Time` (D62 amend): каждая fn, транзитивно зовущая Time-опы (в т.ч. через `Timestamp.now()`/`Duration.@sleep()`/free `sleep`), обязана нести `Time` в effect-row под `--strict-effects`, симметрично Fs/Net/Db. Механическая миграция по diagnostic-loop (`check --strict-effects` → добавить `Time` в сигнатуры по ошибкам → повторить до нуля) через ВЕСЬ std, затем examples, затем conformance-фикстуры, пинующие ambient-поведение — масштаб сопоставим с 755+-сайт retyping'ом из Plan 175 §6 (rename), отдельное окно; НЕ уместилось в 175 Ф.2-v2 вместе с capture-mechanism-фиксом + `#default_handler` + typed-schema-исследованием. | 175 Ф.2-v3/v4 (следующее окно) | P2 |
| `[M-detach-consume-escape-unchecked]` | ✅ **CLOSED** (commit `5065f684d`, 2026-07-22 — сам день находки; доклоузинг подтверждён окном 67 consume-звучность 2026-07-24, эта строка была не обновлена автором фикса, документный лаг). detach ловит use-after-consume escape симметрично `spawn consume` (D415 §4 расширение): match/if-let/while-let arm'ы теперь заводят scope-фрейм для pattern-биндингов в capture-checker'е (`ScopeBinding.linear_pattern`); голый `detach { …consume-var… }` из объемлющего scope → `E_LINEAR_CAPTURE_IN_FIBER`; явный move — новая форма `detach consume x [= expr] { … }` (parse_detach, зеркало parse_spawn). Флагман (`examples/flagship/aggregator/src/main.nv:294`) переписан на `detach consume stream { … }`. Гейты: `neg/detach_consume_escape_neg.nv` (пин, точный флагман-паттерн) + `detach_consume_move_ok.nv` — оба зелёные при верификации 2026-07-24. D-амендмент D415 §2/§4 в `spec/decisions/06-concurrency.md`. | types/mod.rs (consume-analysis, класс D415) | ✅ done |
| `[M-test-runner-tempdir-race-jobs]` | **ЧАСТИЧНО ЗАКРЫТО 2026-07-22 (worktree `nova-tmprace`, ветка `p-fix-tmprace`, sonnet) — гипотеза «shared temp-dir» ОПРОВЕРГНУТА, найден и закрыт УЗКИЙ смежный гап, реальный корень — ОТДЕЛЬНЫЙ баг, вынесен в новый маркер.** Аудит `test_subdir`/`exe_file`/`obj_dir` (`compiler-codegen/src/test_runner.rs`) показал: изоляция уже корректна — process-unique `tmp_dir` (PID-суффикс, фикс 2026-07-07) + per-`display` 64-bit хеш subdir на КАЖДЫЙ job; `.c`-компаньон пишется рядом с исходником, но путь уникален (свой `nv_file` на job), кросс-job коллизии НЕ обнаружено. `detect_or_build_rt_archive` уже под mutex (`[M-218-rt-archive-parallel-jobs-race]`), `detect_or_build_libuv` вызывается ОДИН раз до пула потоков. Реальный узкий гап: RUN-шаг (`Command::spawn()` на уже слинкованный `exe_file`) не имел НИКАКОГО retry на транзиентный Windows exec-lock (AV/Defender скан при первом запуске — `ERROR_ACCESS_DENIED`/`ERROR_SHARING_VIOLATION`, raw OS 5/32), в отличие от CC/link-шага (там уже есть `CC_LOCK_RETRIES`). Закрыт: `is_transient_exec_lock_error` + retry-loop на RUN-спауне (5 попыток, backoff 200мс×N) + opt-in `NOVA_DEBUG_RUN_DUMP=1` (dump exit+tail для будущих RUN-FAIL). 2 новых юнит-теста (`exec_lock_classifies_transient_windows_codes`/`exec_lock_does_not_classify_real_errors`), `cargo test --release --lib test_runner` — 57/57 PASS (искл. pre-existing `p0_erased_now_dispatches_via_vtable` stack-overflow, не связан). **НО:** мега-CU гейт (`nova test --positive --compile-error --jobs 16 spec_tests/conformance`) прогнан 8× ПОСЛЕ фикса — 7 PASS 528/0/55, но 1 RUN-FAIL на `a_q3_println_debug_record` (elapsed 517s < timeout 600s, exit≠0, БЕЗ единой `FAIL:`/`panic:` строки — процесс реально упал mid-run, не lock/spawn). Это ТОЧНО симптом, описанный в исходном маркере, но fix его НЕ устранил (воспроизвёлся даже без внешней нагрузки от других агентов) — совпадает с уже задокументированным (2026-07-13, для другого embedded-теста `app_effect_basic_t8_1` того же mega-CU) паттерном необъяснённого mid-run краха. Диагноз: НЕ temp-dir/output-path race — похоже на load-sensitive concurrency/GC-таймингов баг ВНУТРИ исполняемого test-бинаря (M:N рантайм), вне зоны «минимальная правка test_runner.rs». Вынесено отдельным маркером `[M-conformance-megacu-intermittent-run-crash]` (см. ниже) для выделенного расследования по `docs/mn-coding-conventions.md`/`docs/debugging-races.md`. | compiler-codegen/src/test_runner.rs | P2 |
| `[M-conformance-megacu-intermittent-run-crash]` | **✅ ЗАКРЫТО 2026-07-22 (worktree `nova-mncrash` @ `C:/Users/Public/nova-mncrash` — НЕ на D: (exFAT, кластер 1 МБ, диск полон), ветка `p-fix-mn-crash`, fable).** Корень — НЕ гонка планировщика/GC (три TSan-гонки 211 §7.3/§7.4 закрыты и присутствуют в main — проверено), а **codegen-баг `emit_detach`** (`compiler-codegen/src/codegen/emit_c.rs`): capture-анализ (`by_value = !is_mut`) слал mut-капчеры **by-reference** (`ctx->cap = &stack_local`) — скопировано с emit_spawn, где родитель join'ится до выхода кадра; detach-орфан fire-and-forget переживает кадр → **use-after-return**. Триггер в корпусе: `detach_consume_move_ok.nv` (добавлена 2026-07-22 утром, 5065f684d — потому «изредка» началось именно в той волне; краш 2026-07-13 — тот же класс) — `mut inflight = AtomicInt.new(0)` в test-теле + `detach { …; inflight.fetch_sub(1) }` без drain: при задержке старта орфана под нагрузкой он читал мёртвый стек → мусорный хэндл → AV-WRITE в `fetch_sub` (или тихая порча чужой кучи — вторая мода, наблюдена как `arr.push` a[2]≠3). **Диагностика по плейбуку**: exe напрямую (соло 4.2s — 517s из симптома были компиляцией под 16 jobs), репро 4-way parallel = **6/40 крахов (p≈15%)**, `NOVA_DIAG_SEGV=1` VEH frame[1] = `_nova_detach_1` → `Nova_AtomicInt_method_fetch_sub_int` (6/6 идентичны). **Фикс**: heap-box mut-капчеров в emit_detach (идиома escaping-handler: ленивый GC-box + `var_boxed`-регистрация; тип ctx-поля и тело орфана не тронуты) — D50 §3.1 drain-видимость сохранена, D415 §2 share-гейт делает box-копию хэндла семантически точной. **Регресс-фикстура** `spec_tests/conformance/detach_mut_capture_outlives_frame.nv` (детерминированная: sleep(60ms) в орфане + смерть кадра; pre-fix RUN-FAIL, post-fix PASS). **Верификация**: 48/48 прямых 4-way parallel чисто (при p=0.15 ложная чистота ≈0.04%); мега-CU гейт `--jobs 16` **×10 подряд зелёный** (528/0/55, 0 RUN-FAIL, 0 SEGV); std/src/concurrency 4/0; supervisor_parfor/stop PASS; флагман-агрегатор собран `--strict-effects`. Коммиты: e72a58170 (диагноз), 47ad72aa5 (фикс), db6dd4f71 (фикстура). **Ещё одно независимое наблюдение pre-fix (2026-07-22, worktree `nova-2171`, план 217.1, sonnet):** тот же RUN-FAIL на том же `a_q3_println_debug_record` (elapsed 1211.9s, exit≠0, без `FAIL:`/`panic:`) на диффе, целиком не пересекающемся с прошлым repro (только `std/src/net/{tcp,udp}.nv` + фикстуры/доки); немедленный повтор без изменений — чистый 528/0/55. Согласуется с найденным корнем (load-sensitive use-after-return в detach, не привязан к дифф-содержимому). *(Исходная запись: найдено 2026-07-22 при закрытии `[M-test-runner-tempdir-race-jobs]`, worktree `nova-tmprace`.)* `spec_tests/conformance` mega-CU (`nova test --positive --compile-error --jobs 16 spec_tests/conformance`) изредка (~1 из 8 прогонов в ЭТОЙ волне, БЕЗ внешней нагрузки от других агентов) даёт RUN-FAIL на своём entry `a_q3_println_debug_record`: elapsed 517s (< timeout 600s), exit≠0, ни одной `FAIL:`/`panic:` строки в stdout/stderr до конца — процесс реально падает mid-run. Изоляция test_runner.rs (temp-dir/exe/obj per-job, rt-archive mutex) аудирована и подтверждена корректной — НЕ инфра-баг. Совпадает с уже известным (2026-07-13, комментарий в test_runner.rs про `app_effect_basic_t8_1`) паттерном для того же mega-CU. Гипотеза: load-sensitive concurrency/GC-race ВНУТРИ M:N рантайма самого исполняемого теста (16 параллельных workers конкурируют за CPU → тайминг-чувствительный сценарий где-то среди 528 embedded test-блоков). Диагностика: `NOVA_DEBUG_RUN_DUMP=1` (добавлено этой волной) даст exit-код+tail на следующий repro. Требует отдельного расследования по `docs/mn-coding-conventions.md`/`docs/debugging-races.md` (race-state-dump протокол), НЕ входит в scope «test_runner.rs минимальная правка». **ЭСКАЛАЦИЯ 2026-07-26 (интегратор, окно 222.3 Гэп-2, верификация): P1→P0 — найден системный МАСКИРУЮЩИЙ РИСК, не только частота.** На ЭТОЙ машине частота сейчас 100% (согласуется с независимым наблюдением №131 тем же днём, «6/6»). Верификация Гэп-2-фикстуры (`gap2_arity_sibling_static_protocol_dispatch_ok.nv`) вскрыла: (1) `nova test <единственный-файл-внутри-папки>` НЕ изолирует — компилирует/исполняет ВСЮ папку `spec_tests/conformance` как один CU/процесс (подтверждено: standalone-прогон одного нового файла печатает PASS-строки полусотни ДРУГИХ файлов — d55/d58/d59/d60/d61 — перед креш-меткой); (2) при явном временном исключении `a_q3_println_debug_record.nv` из CU (файл вынесен и возвращён) НОВАЯ фикстура ВСЁ РАВНО крашится независимо — `NOVA_DEBUG_RUN_DUMP=1` даёт `exit=-1073741819` (0xC0000005 ACCESS_VIOLATION), НИ ОДНОЙ `FAIL:`/`panic:` строки — то есть минимум ДВА независимых источника одного и того же RUN-FAIL-симптома (a_q3 — НЕ единственный). **КРИТИЧНО:** в `--jobs N`-шардированном прогоне процесс, упавший на КАКОМ-ТО тесте, не продолжает очередь СВОЕГО шарда — любой тест, зарегистрированный ПОСЛЕ крашащего в порядке исполнения того же шарда, НИКОГДА не выполняется, но и не считается ни PASS, ни FAIL — общий счётчик (`PASS: N FAIL: M SKIP: K`) не отражает пропуск (нет ассерта «N+M+K == ожидаемое число файлов»). Значит любое `мега-CU 586/1/67`-подтверждение этой сессии МОГЛО тихо не исполнить новые фикстуры, попавшие в тот же шард ПОСЛЕ падающего теста — ложно-зелёный гейт по форме, неполный по факту. Рекомендация: (а) `test_runner.rs` должен ассертить сумму PASS+FAIL+SKIP против числа обнаруженных .nv-тест-файлов и громко падать при расхождении; (б) до полного фикса — поведенческие/новые фикстуры проверять ДОПОЛНИТЕЛЬНО прямым стендалон-прогоном с временным исключением уже известных крашащих файлов (см. приём выше), не доверять только агрегату мега-CU. | nova_rt concurrency (M:N runtime) / test_runner.rs (маскировка) / spec_tests/conformance | **P0** |

| `[M-176-memfs-gc-pressure]` | Runtime: 10-тестовый `mock_fs` binary (тяжёлый `MemFs` c parallel-Vec + Vec-of-Vec captured в effect-handler) даёт нон-детерм. сбой на write_atomic-тесте под GC-давлением (isolated 4/4 PASS; в 10-тестовом binary — flaky/fail). io/net-mock (мельче) стабильны. **Обход:** тесты 176 разбиты на файлы ≤3 теста (mock_fs/mock_options/mock_dir/mock_atomic) — каждый binary лёгкий, стабилен. Root — GC-трейс большого captured heap-record под давлением; investigate. | Plan 176 Ф.2 / runtime-GC | P2 |
| `[M-176-cwstr-direct-winapi]` | CWStr (newtype над `*u16`) для прямого `CreateFileW`/`_wopen` НЕ введён: libuv `uv_fs_*` принимает UTF-8/WTF-8 `const char*` и сам конвертит в UTF-16 на Windows → CWStr-маршалинг не нужен на libuv-бэкенде. Понадобится только при прямом Win32-биндинге (не через libuv). 174.6 §2 C_ABI-грамматика newtype-правила — отложены соответственно. | Plan 176 Ф.2 / 174.6 | P3 |
| `[M-176-cstr-from-bytes-canonical]` | §3c `CStr.from_bytes(bytes)->Result[CStr,IoError]` реализован как локальный `c_path(path []u8)->Result[[]u8,IoError]` в std/fs (reject interior-NUL + NUL-terminate; []u8-native). str-buffer `CStr` (std/ffi/cstr) — отдельный примитив (str-view, не []u8). Канонизировать `CStr.from_bytes` в cstr.nv (требует io-dep) — followup. | Plan 176 Ф.2 | P3 |
| `[M-176-dir-scoped-ops]` | Zig openat-модель (anti-TOCTOU by design): `remove_dir_all`/walk через `openat`/`unlinkat`+NOFOLLOW вместо path-based recursion (symlink-race-safe). Текущий `remove_dir_all` — простая рекурсия. | Plan 176 followup | P3 |
| `[M-176-create-temp]` | Уникальные temp-имена (O_TMPFILE/anonymous): `write_atomic` использует детерм. sibling-имя `.<name>.novatmp` (коллизия только при конкурентном atomic-write того же таргета). create_temp API — followup. | Plan 176 Ф.2 followup | P3 |
| `[M-176-generic-wrapper-mono-inference]` | ✅ **CLOSED 2026-07-06.** Codegen: generic wrapper-тип с void-ptr-полем (bare type-param / protocol-bounded / typed-ptr — stub-условие `has_void_ptr_fields`) при inference-only конструкции (`BufWriter.new(src)`, W из аргумента) эмитил NULL-stub `Nova_<T>_static_new(void*){return NULL}` + держал LHS-локал в erased-типе → методы уходили в erased no-op → CC-FAIL/крах. Fix (codegen-only, mirror turbofish static path): `try_generic_static_ctor_mono` выводит type-args из ctor-args, регистрирует mono-instance+worklist, диспетчит в `Nova_<T>____<args>_static_<m>`; `infer_generic_static_ctor_ret` — inference-двойник (mono-instance C-тип, воткнут в НАЧАЛО `infer_expr_c_type` до checker-каналов — checker резолвит inference-ctor в ERASED wrapper без type-args). Оба gated на `generic_type_has_voidptr_fields` (stub-only) + полную выводимость. Test: `nova_tests/plan176_holes/m176_wrapper_ctor_inference.nv`. Zero-regression (~41 CU vs merge-base). | Plan 176 Ф.1 / codegen | ✅ done |
| `[M-valuerecord-result-vtable-mono]` | ✅ **CLOSED 2026-07-04.** Codegen keystone: `Result[T, <value-record E>]` в возврате protocol-метода / generic-fn / generic-wrapper-метода → CC-FAIL `unknown type name 'NovaRes_<ok>_NovaValue_<E>'`. Root: protocol-vtable struct эмитится в РАННИЙ буфер (`user_type_fwd_decls`/`generic_type_defs`) и ссылается на КОНКРЕТНЫЙ `NovaRes_<ok>_NovaValue_<E>*` (в отличие от heap-error, который `type_ref_to_c` стирает в рантайм-предопределённый `NovaRes_nova_int_nova_str`), а typedef этого mono splice'ится позже (`__NOVARES_TYPEDEFS__`) → «unknown type» на vtable. Fix: `emit_protocol_box_typedef` forward-declare'ит `typedef struct NovaRes_<n> NovaRes_<n>;` референсимых value-record-`NovaRes` monos в ту же раннюю зону ДО своей struct'ы (pointer-field → достаточно tag; полное тело — прежним phase-correct splice'ем; C11 6.7/3 redundant-typedef). Это разблокировало `IoError` heap→`value` (176 §3b), value-record error в generic-wrapper с явными type-args (repro g1/g2x). Разблокирует 180 (SerError/DeError value), потенц. 178 (HttpError). | Plan 176 / codegen | ✅ done |
| `[M-valuerecord-receiver-generic-method]` | ✅ **CLOSED 2026-07-06.** Codegen: метод-generic метод (`fn VDec mut @take[T](x T) -> Result[T, DeErr]`) на value-record РЕСИВЕРЕ. Receiver-ABI (`&d` для mut value-record) уже был корректен (prior fix); осталась call-site return-inference: `__mono_method__`-sentinel-путь в `infer_expr_c_type` лоуэрил подставленный return через value-БЛЕДНЫЙ static `apply_type_subst_to_ref`, который ПРОПУСКАЕТ `Result`/`Option` (у них спец-мангл `NovaRes_`/`NovaOpt_`) → `Result[T, <value-record>]` уходил в `void*` fallback → match-scrutinee `void*` (`_nv_scr->tag`/`payload.Ok` на void → CC-FAIL) + Ok-binding `Nova_T*`. Fix: sentinel-путь резолвит return через `resolve_result_option_ret` (строит+регистрирует `NovaRes_<ok>_NovaValue_<E>*`), fallback → value_aware → static. Оба copy Call-арма. Test: `nova_tests/plan176_holes/m176_valuerecord_receiver_generic_method.nv`. | Plan 180 / codegen | ✅ done |
| ~~`[M-176-xmod-payload-variant-ctor]`~~ | ✅ **CLOSED 2026-07-14** (ветка `fix-m176-xmod-variant-ctor`, sonnet). P67-panic-часть закрыта. Root: чисто ПАРСЕРНАЯ форма — PascalCase path-collector (parser/mod.rs `starts_uppercase`) жадно сворачивает каждый `.ident` после начального PascalCase-сегмента в один плоский `Path` (нужно, чтобы `SeekFrom.start(5)` парсился одним Path), поэтому nullary-цепочка `Type.Variant.method()` (без скобок на варианте) схлопывается в `Path([Type,Variant,method])`, обёрнутый одним Call — минуя Member-диспетч, который проходит связанная `ro x = Type.Variant; x.method()`. Плоский Path потом падал в `[P67-LEGACY] ... (no method segment)` (хард-паника, роняет весь merged-CU). Payload-ctor (`Type.Info(5).method()`) НЕ схлопывается — скобки ctor'а уже дают Member-форму. Fix (`variant_chain_as_member`, emit_c.rs): в начале ОБОИХ `infer_call_ret_c` и `emit_call` схлопнутая форма переписывается в эквивалентный `Member{ Path[Type,Variant], method }` и повторно входит — collision-aware И generic-aware проверка варианта (голое имя ∨ `ref_type_base`-квалиф ∨ generic-template variants). Concrete/colliding/generic резолвятся через существующую Member-машинерию, ноль дублирования; ни одного P67-пути. Test: `spec_tests/conformance/m176_variant_ctor_method_test.nv` (+ fixtures `_a`/`_g`). **Остаточный дефект (НЕ P67, отдельный):** `[M-176-collision-variant-method-dispatch]` ниже — метод на variant-цепочке (И на `ro`-форме одинаково, доказано flat≡ro) мис-диспатчится при коллизии имени суммы D381 + разделяемом имени метода. | Plan 176 Ф.1/Ф.4 / parser+codegen | ✅ done |
| `[M-176-collision-variant-method-dispatch]` | Codegen (D381 Member-dispatch): вызов метода на variant-значении (`Type.Variant.method()` ИЛИ `ro x = Type.Variant; x.method()` — **обе формы врут ОДИНАКОВО**, доказано flat≡ro в диагностике 2026-07-14) при ОДНОВРЕМЕННОМ выполнении двух условий даёт НЕВЕРНОЕ значение (не паника, не CC-fail): (1) имя суммы `Type` КОЛЛИДИРУЕТ (D381, объявлено в ≥2 модулях merged-CU) → C-тип ресивера квалифицируется (`Nova_<qual>_<Type>*`), (2) имя метода РАЗДЕЛЯЕТСЯ несколькими типами в CU → срабатывает single-key last-wins fallback (`method_receivers`). Механизм: квалифицированный C-тип ресивера промахивается по `method_overloads` (keyed по ГОЛОМУ имени типа), падает в name-keyed last-wins → выбирает ЧУЖОЙ одноимённый метод. Без коллизии ИЛИ без разделяемого метода — корректно (проверено: unique-Type ∨ unique-method → PASS обе формы). Пре-существующий (не зависит от variant-chain rewrite; ro-форма затронута идентично). Repro-диагностика: три `Kind` (m176_a/xmodule_a/xmodule_b) + decoy `@label`/`@name` → и flat, и ro врут. Фикс: method-dispatch на квалифицированном ресивере должен искать `method_overloads` по ГОЛОМУ base-имени типа (методы регистрируются под `recv.type_name` = голое имя) ЛИБО регистрировать методы под квалифицированным именем для колл-типов; убрать name-keyed last-wins из горячего пути для типизированного ресивера. | Plan 176 Ф.4 / codegen (D381) | P2 |
| `[M-175-value-record-const-ref]` | ⚠️ **ЧАСТИЧНО РЕШЕНО 2026-07-31 (worktree `nova-p157`, ветка `p157-ro-assoc-const`, sonnet, [Plan 157](221.1-bug-sweep.md) §157, A-V8 §2 зона 1).** Root уточнён: не codegen-баг emission/DCE, а **декларация не в том синтаксисе** — `Duration.ZERO` был объявлен КАК bare module-level `export const ZERO Duration = {…}` (голый символ `ZERO`), а ссылка `Duration.ZERO` резолвится как D200 out-of-body assoc-const (ожидает мангл-символ `Duration_ZERO`) — отсюда mismatch. **Фикс (не codegen, а std-миграция на уже существующий, корректно работающий D200 out-of-body синтаксис):** `ZERO`/`SECOND`/`MINUTE`/`HOUR` в `std/src/time/duration/core.nv` переписаны в каноническую qualified-форму `export const Duration.NAME Duration = {…}` — `Duration.ZERO` теперь резолвится по-настоящему; call-сайт `monotonic.nv:131` (`0.to_nanos()`) и doc/test-обходы в `core_test.nv` (2 сайта) мигрированы на прямую ссылку `Duration.ZERO`. Гейт: `nova test std/src/time` PASS 6/0 (SKIP 8 — compile-error/runtime-panic lanes, не связаны). **Остаётся ОТКРЫТЫМ (НЕ покрыто этой волной):** (1) `civil/date.nv:35`/`civil/time_of_day.nv:29` — `Date.MIN`/`Date.MAX` static-fn обход держится ТАКЖЕ на `[M-175-type-const-max-shadows-builtin]`-соседе (constant по имени `MIN`/`MAX` конфликтует с builtin numeric `.MIN`/`.MAX` в type-set generics — отдельный, отдельно закрытый в 2026-07-14 гейт КОНКРЕТНО для type-инференции, но не проверено для НОВОГО assoc-const пути), маркеры оставлены; (2) корневая систем-дыра «bare top-level `const NAME = …` vs qualified-ref `Type.NAME`» сама по себе НЕ зачинена в codegen — она просто больше не ЗАДЕТА для Duration, потому что декларация переписана в корректный синтаксис; тот же класс бага воспроизведётся для ЛЮБОГО будущего bare-const, если кто-то попробует сослаться на него через `Type.NAME`. Ре-открытие оставлено как явный follow-up, если решится чинить codegen-уровень, а не только миграцией сайтов. | Plan 175 Ф.1b / std-миграция (Plan 157) | ⚠️ P2 (частично) |
| `[M-175-value-in-generic-tuple-return]` | Codegen: generic-fn, возвращающая tuple с value-record-элементом (`fn measure[T](…) -> (T, Duration)`), на call-site (`measure(||…)`) инферит return как legacy `_NovaTuple2` (оба слота erased→nova_int) вместо mono `_NovaTuple_2_…_NovaValue_Duration` → temp-slot type-mismatch + destructure даёт `nova_int elapsed`. Value-record-специфичный (обнаружен Plan 175 Ф.1b; `measure` не в корпусе → не блокер). **Обход:** избегать generic-tuple-destructure с value-элементом. Фикс: протянуть mono'd return-tuple-тип generic-fn в `infer_expr_c_type(Call)`. | Plan 175 Ф.1b / codegen | P2 |
| ~~`[M-175-type-const-max-shadows-builtin]`~~ | ✅ **CLOSED 2026-07-14** (ветка `fix-m175-maxmin-shadow`, sonnet). Root: `infer_expr_c_type`'s Path arm (emit_c.rs) never substituted a type-PARAMETER `parts[0]` through `subst_c` (mono `current_type_subst`) — only the sibling VALUE-emission path (`emit_expr`, D310/Plan 172.3) did. Missing that mirror, `T.MAX`'s TYPE fell through to the legacy final Path arm's bare `var_types.get(last)` (last segment `"MAX"`), which resolves the constant NAME against ANY module-level const of that name regardless of qualifier — hence a user `const MAX Duration = {…}` shadowed the builtin. Fix: added the same `subst_c` → Nova-primitive → `numeric_type_constant_mapping` substitution on the TYPE side, gated identically to the existing VALUE-side D310 code (only fires when the substituted concrete type is itself a numeric primitive — a concrete type qualifier like `Duration.MAX` never matches `current_type_subst`, so legit assoc-const reads are untouched). Conformance regression: `spec_tests/conformance/m175_maxmin_shadow_builtin.nv` (reuses `D310Ints`); `d310_type_set_bound.nv` verified unaffected (targeted isolated-CU rebuild, byte-for-byte same test bodies, all PASS). **Блокирует publicly вводить `Duration.MAX`/`Duration.MIN`-консты снят** (Plan 178 `@timeout(Duration.MAX)`) — консты сами НЕ введены в этой волне (не обязательно; `[M-175-value-record-const-ref]` — отдельный, всё ещё открытый codegen-гэп для value-record const-AS-VALUE — блокирует их полноценное введение независимо от этого фикса). | Plan 175 Ф.1c / checker | ✅ done |
| `[M-sleep-tolerance]` | Swift-style `tolerance:`-параметр у `sleep`/`sleep_until` (энергоэффективность/timer-coalescing — ОС может сдвинуть пробуждение в пределах tolerance для батчинга таймеров). Сигнатура `sleep` future-proof под аддитивное добавление (Ф.4). Только Swift среди peers имеет tolerance. Вводить при use-case (power-sensitive workloads). | Plan 175 Ф.4 followup | P3 |
| `[M-monotonic-boottime]` | ContinuousClock-аналог: монотонные часы, ВКЛЮЧАЮЩИЕ время сна устройства (`CLOCK_BOOTTIME` Linux / `QueryInterruptTime` Win / macOS continuous). `Monotonic` (D318) = uv_hrtime (suspend-EXCLUDED). Индустрия расходится (Zig=BOOTTIME, Rust/Go=MONOTONIC, Swift=оба). Вводить при use-case «дедлайны через сон устройства». | Plan 175 followup / D318 | P3 |
| ~~`[M-176-io-forward-bounded-generic]`~~ | ✅ **CLOSED 2026-07-31** (worktree `nova-p176`, ветка `p176-io-forward`, sonnet). Checker: форвард bounded-generic параметра в другую generic-fn с тем же bound (`fn f[R Read](r){ read_to_end(r) }`) → «R does not satisfy Read» (bound не проносился через форвард). **Root уточнён разведкой:** к моменту фикса прямая репродукция УЖЕ не воспроизводилась — но по ОБРАТНОЙ, более опасной причине: пассthrough-скип [M-property-testing-rot] (Plan 172.13 батч 3, `current_fn_generic_names: HashSet<String>`) сверял concrete-имя аргумента ТОЛЬКО по имени, без учёта bound'а, из-за чего форвард БЕЗ bound'а у caller'а (`fn f[R](r){ read_to_end(r) }`, R вообще без bound) ТОЖЕ молча проходил — честная ошибка терялась (verified regression до фикса). **Fix** (`types/mod.rs::BoundCtx::check_satisfaction`): `current_fn_generic_names` → `current_fn_gs: RefCell<GenericScope>` (Plan 196 gs — имя → полный `GenericParam` с bounds, заполняется переиспользуемым `fn_generic_scope(fd)`); skip срабатывает, только если ОДИН из bound'ов caller'а реально покрывает bound callee (exact-match, либо protocol-суперсet через новый `generic_bound_covers`, переиспользующий уже DFS-flattened `protocol_specs` — никакой новой иерархии). Иначе — честный fall-through, typevar не имеет `method_table` → корректно репортит missing. **Обход снят:** `std/src/io/buffered.nv`'s `BufWriter[W Write] mut @drain()` — инлайн-копия `write_all`-цикла заменена форвардом `write_all(@inner, @buf)` (метод→свободная, ТОТ ЖЕ дефект-класс). **НЕ снято (иные причины, не этот дефект):** `std/src/fs/fs.nv`'s `read`/`write` (concrete `File`-ресивер, не generic — never blocked by this bug; инлайн там ради must-consume+close-error-precedence combining, дизайн-выбор); `std/src/encoding/json.nv` и `std/src/text/regex.nv` (аудит A-V8 устарел — ни один не ссылается на `std.io`/bound-forward вообще); `std.http.serdejson`'s `json_as`/`json_decode_body` (упомянуто в `[M-codegen-method-return-turbofish]` выше как «отдельный открытый bound-forward gap» — модуль/файл в ЭТОМ репозитории `std/` отсутствует, вероятно nova-http, отдельный репозиторий — вне scope этого worktree). Фикстуры: `spec_tests/conformance/m176_io_forward_a/` + `m176_io_forward_test.nv` (pos: свободная×свободная через границу модуля, метод→свободная, цепочка трёх, регресс δ0) + `spec_tests/conformance/neg/m176_io_forward_no_bound_neg.nv` (neg: форвард без bound'а — честная ошибка). | Plan 176 Ф.1 / checker | ✅ done |
| `[M-83.10.4-iso-cancel-startup-race]` | ✅ **CLOSED 2026-06-11 (Plan 83-go-cmn Ф.5).** Структурно закрыт Ф.2 (gopark timer-backed park вето́ит на cancel перед arming + READY-latch + driver async-close wake) — НЕ потребовался отдельный код. Подтверждено: design-workflow 380 armed-прогонов + мои 320 (160@MP=1 + 160@MP=4) = 0 hang. 3 disabled stress-теста (supervised_cancel_stress_test) re-enabled с wake-not-hang бюджетами (исходные latency-SLA флакали ~0.8% под jitter, не hang). | Plan 83-go-cmn Ф.5 | ✅ done |
| `[M-83-gopark-bare-park-cancel-veto]` | Теоретический gap: `nova_gopark` НЕ имеет cancel-veto перед WAIT-store. Timer-backed park (Time.sleep) не зависит от него (driver async-close wake → cancel-check в yield), поэтому iso-cancel закрыт. Но **bare gopark без stop_cb** (channels/net — channels.h ~1012, net.c) теоретически мог бы park'нуться с уже-выставленным cancel. Не воспроизведён (нет фикстуры). Fix если всплывёт: Go-style cancel_requested re-check в gopark до WAIT (композится с READY commit-recheck). | Plan 83-go-cmn (Ф.3+) | P3 |
| `[M-83-gocmn-note-primitive-deferred]` | Ф.3 design-finding: `uv_async` УЖЕ корректный note (idempotent + IOCP-backed Windows) → собственный `note.h`/Go lock_sema НЕ нужен. Понадобится только если Ф.6 (timer-heap) / Ф.8 (netpoll) уберут libuv из worker-park. | Plan 83-go-cmn (Ф.6/Ф.8) | P3 |
| `[M-83-f3-coalesce-gated-on-f4]` | Ф.3 nspinning wakep-coalescing (пропустить uv_async_send когда spinner найдёт работу) НЕБЕЗОПАСЕН в текущей per-worker `wake_pending` топологии (spinner не дренит чужой wake_pending → lost-wakeup, review GAP-1/2/3). **Gated на Ф.4** (global-queue routing — spinner сканит global → coalesce-safe). Порядок: Ф.4 → Ф.3-coalesce. | Plan 83-go-cmn Ф.4→Ф.3 | P2 |
| `[M-83-f4-global-routing-gated-on-bench]` | Ф.4 global-routing (cross-thread работа → global вместо home-worker wake_pending) ОТЛОЖЕН: review нашёл stranding (wake-one неверен; нужен wake-all) + home-affinity дизайн Nova уже корректен/stranding-proof. Go global-queue = balancing-vs-locality tradeoff, НЕ строгое улучшение. Делать только если профайл покажет home-affinity боттлнеком. Блокирует Ф.3 coalescing. Безопасный subset (steal random-victim + 61-tick fairness) — минор. | Plan 83-go-cmn (bench-gated) | P3 |
| `[M-83.11-grow-vs-wake-race]` | ✅ **CLOSED 2026-06-11** (Plan 83-go-cmn Ф.1b, commit `e1525d90671`). Структурный фикс: `NovaSchedState` chunked stable-address storage (chunk'и never-realloc → torn-pointer невозможен); GitHub issue #2. Closure: grow_vs_wake_explicit 100/100 + stress_iso_3e 66/66 + semaphore_batch_n 30/30 armed. История — simplifications.md + plan §9.5. | Plan 83-go-cmn Ф.1b | ✅ done |
| `[M-debug-line-directives]` | Нет `#line N "file.nv"` → дебаггер показывает C, не Nova. Только comment-only `/* SRC */`. | Plan 25 G9 → dedicated план | P1 |
| `[M-173-error-return-trace]` | ✅ **CLOSED 2026-07-13** (волна «173 хвосты», ветка `tails-173`). Полный propagation-trace поверх Ф.5-минимума `cdd23a5b2`: TLS ring-buffer `_nova_throw_trace` (16 записей + счётчик вытесненных) в effects.h/effects.c; codegen-push на каждой `?`-точке (value-mode `return Err` + Fail-ctx конверсия) и на `!!`-Err (конверсия пропагирующей ошибки — `nova_throw_site_mark` обновляет site БЕЗ сброса трассы); `!!`-None = свежий origin (`site_set`). Сброс трассы: fresh throw-origin (`site_set`) / catch (`nova_scope_exit` CATCH, interrupt-consume effects.c) / `nova_runtime_reset`. Дамп во всех uncaught-abort ветках: `propagation trace (?-chain, oldest first)` + `via file:line (?)`. Тест `nova_tests/err173/rt/f5_propagation_trace_full.nv` (3 звена, порядок, --panic lane). Известный остаток (задокументирован в effects.h): `Err(...)`-конструктор без throw трассу не сбрасывает — кадры разобранной match'ем ошибки могут остаться в хвосте следующего дампа. | Plan 173 (хвост) | ✅ done |
| `[M-cli-build-source-file-name-unknown]` | Обнаружено волной «173 хвосты» (2026-07-13): путь `nova build` (nova-cli) НЕ вызывает `set_source_file_name` у эмиттера → throw-site/propagation-trace печатают `at <unknown>:12` (line честный, файл нет). Путь `nova test` (test_runner.rs:3457) и nova-codegen main.rs:517 стемпят basename честно. Pre-existing (виден и на Ф.5-минимуме до этой волны). Fix: прокинуть basename entry-файла в build-пайплайне nova-cli. | floating (nova-cli) | P3 |
| `[M-cas-return-witnessed-value]` | ✅ **CLOSED 2026-07-15** (Plan 207, sonnet). Все 13 CAS-методов (`AtomicI8..I64`/`U8..U64`/`Isize`/`Usize`/`Ptr`/`Bool`/legacy `AtomicInt`) переведены `bool` → `Result[(), T]` (`Ok(())` успех / `Err(actual)` провал, `actual` = witness даром из C11 `__atomic_compare_exchange_n`, без повторного `load()`). Лоуэринг: private `@cmpxchg` extern intrinsic (strong+weak делят один intrinsic через `weak bool` параметр) возвращает raw `(ok, witness)` value-struct (`CasRaw*` named-tuple, 11 witness-ширин); публичный `compare_exchange`/`_weak` — plain `.nv` wrapper, строит `Result` обычным Nova-конструктором (без hand-written Result C). Потребовался codegen-фикс (`emit_c.rs` `RUNTIME_DEFINED_TYPES`-ветка `emit_type_decl`): NamedTuple-типы в этом списке теперь регистрируют field-schema без повторной эмиссии struct-body (раньше ветка знала только `Sum`/`Effect`). Спека: [D425](../../spec/decisions/06-concurrency.md#d425-cas-возвращает-свидетеля-провала-compare_exchange-bool--resultt-plan-207) (amends D168 §1). Call-сайты: `sync_test.nv` (+ новый witness-тест), 10 `spec_tests/conformance/*.nv`. Гейты: conformance 150/0, `sync_test` PASS (test-build). | Plan 207 | ✅ done |
| ~~`[M-194-verified-mode-gate]`~~ | ✅ **ОТМЕНЁН 2026-07-15** (владелец): режим `--contracts=verified` (whole-build «докажи всё или compile-error») — footgun (половину контрактов пришлось бы в ручной `if`/assert). УДАЛЁН из флага (`checked|optimized` только; enum `ContractsMode` без `Verified`; D421 §3-амендмент). Статическая верификация остаётся через per-fn `#verify` + `nova verify-contracts` — не build-режим. Не «отложен», а именно отменён. | Plan 194 (закрыт) | ✅ |
| `[M-closurefull-let-empty-ty]` | Обнаружен волной handler-annot (2026-07-10): let-bound ClosureFull (`ro f = fn(a int) -> int => a+1`) → CC-FAIL «use of undeclared identifier f» В ЛЮБОМ контексте, вкл. обычные тест-тела. Root: `infer_expr_c_type(ClosureFull)` возвращает пустой `ty_c` → Let-decl эмитится без типа (` f = (void*)&nova_lambda_N_clos_singleton;`). Канал: чекер пишет `resolved_types` (`ResolvedType::Func` → `NovaClos_X*`) только для zero-param ClosureLight (types/mod.rs:8424-8443); ClosureFull-ветка (:8445) не аннотирует, а в `infer_expr_c_type` legacy-ветки для ClosureFull нет (есть ClosureLight :49034 и Lambda :49102). **Pre-existing** (чистая база 6582887e1 падает идентично; репро: `nova_tests/basics/functions.nv`). Fix-направление: аннотировать ClosureFull в чекере (params+ret явные — гадать нечего) ИЛИ legacy-ветка ClosureFull в infer по образцу Lambda. | floating (codegen/checker, класс 172.12) | P2 |
| `[M-folder-module-spawn-const-capture]` | Обнаружен Plan 175 Ф.1 (2026-07-04): в folder-module `spawn`/`select`-closure, захватывающий **module-level const** (напр. `Duration.from_millis(TIMEOUT_MS1)` внутри `spawn`), эмитит captured-поле по **bare-имени** (`_ctx->TIMEOUT_MS1 = &TIMEOUT_MS1`) вместо мангл-имени `Nova_const_<mod>_TIMEOUT_MS1` → C-compile `use of undeclared identifier`. Делает целый модуль некомпилируемым при `nova test <folder-module-dir>`. **Pre-existing** (baseline-delta=0: тот же CC-FAIL на parent-бинаре). Репро: `nova_tests/plan65` (f10/f7/f11 — `TIMEOUT_MS1`/`TIMEOUT_MS2`/`TIMERS_PER_FIBER`), `nova_tests/basics/control_flow` (`apply`). Fix: спавн-capture должен резолвить module-const в мангл-глобал (не local-var capture). | floating (codegen) | P2 |
| `[M-detach-transitive-effect]` | Обнаружено research-заходом detach vs #blocking/#realtime (2026-07-11): `check_callee_effects` (types/mod.rs:17400) проверяет `callee.effects` только против forbid/realtime/blocking-body — транзитивного «caller обязан объявить `Detach`, если callee его декларирует» НЕТ. Следствие: `fn f() -> () { helper() }` при `fn helper() Detach -> ()` компилируется без `Detach` — сирота прячется на глубине 1 вызова; `forbid Detach` дыряв (обёртка без `Detach` в row обходит sandbox). Fix: Detach-ветка в `check_callee_effects` с теми же exemptions, что `E_DETACH_REQUIRES_EFFECT` (declared_effects / ambient with-handler / effect_root), тот же код диагностики. Закрывает и forbid-дыру. | floating (checker, Plan 173 follow-up) | P2 |
| `[M-detach-with-handler-or-drop-exemption]` | Исключение «ambient `with Detach = …`» в `E_DETACH_REQUIRES_EFFECT` (types/mod.rs:17328) — мёртвая поверхность: хендлер-значений для `Detach` не существует (0 употреблений `with Detach` в std/nova_tests; AsyncDetach/SyncDetach — ретрактированные имена Plan-83-эры). Либо шипнуть реальный тест-хендлер в std/testing (синхронное выполнение тел / сбор для drain — ниша фиктивного SyncDetach), либо убрать exemption из чекера до его появления. | floating (std/testing + checker) | P3 |
| `[M-detach-forbid-test]` | `forbid Detach` (sandbox от сирот, D63×D50) не покрыт ни одним тестом (0 в nova_tests). Добавить pos/neg: `forbid Detach { вызов fn с Detach в row }` → error (`check_callee_effects` forbid-ветка это уже умеет — нужен именно тест); после `[M-detach-transitive-effect]` — и глубокий кейс (обёртка без row). | floating (tests) | P3 |
| `[M-83-study-go-c-mn]` | Порт рабочего M:N из Go ≤1.4 C-рантайма. **✅ research+8-фаз декомпозиция; ✅ Ф.1a ring-port; ✅ Ф.1b chunked park-state (закрыл grow-vs-wake); ✅ Ф.2 gopark/goready (D244, удалил pending_wake, commit d2830c73d7d).** OPEN до Ф.3-Ф.8 (nspinning/iso-cancel/timer-heap/sysmon/netpoll). | Plan 83-study-go-c-mn | P1 |
| `[M-83.11-f2-arm-tsan]` | Ф.2 gopark G0(RELEASE)/G1(SEQ_CST) x86-корректны (XCHG дренит store-buffer); для ARM/weak-memory валидировать под TSAN на Linux. Не регрессия (x86 целевая). Linux-build больше не блокер — см. [docs/linux-build.md](../linux-build.md) (2026-07-16, verified WSL2). | floating (Linux-CI) | P2 |
| `[M-145-msvc-remaining-stmt-expr]` | ✅ **в основном ЗАКРЫТ** (Plan 145 + 145.1 поверх 145.2-детерминизма, 2026-06-15). index/slice/box/bitcast (Plan 145) + struct-write (memcpy-в-слот) + heap-box-примитива (scalar compound literal) + Option-get composite (`(NovaOpt_X){.value=nova_npo_from_tagged_int(...)}`) → portable; record-invariant был false-positive. MSVC ВСЁ зелёное (plan145/138/138_1/91_fe1/90/96/131/152_1/basics/generics), clang 0 net-new. **Site-targeted регресс (добор 2026-06-15):** каждый из 3 закрытых сайтов теперь имеет co-located фикстуру, сверяющую портируемый хелпер в `.c` + PASS clang+MSVC через релизные nova-codegen+CLI — `plan145/t2_index_write_pos` (struct-write `memcpy`), `plan145_2/throw_primitive_expr_pos` (heap-box-примитив `nova_box_value`), `plan145_2/composite_get_option_pos` (Option-get `nova_npo_from_tagged_int`). **ОСТАТОК (узкий):** value-record throw-**как-выражение** (`x ?? throw SomeValueRecord{}`) — rvalue в expression-контексте, hoist небезопасен, compound-literal member-init не годится для multi-field; остаётся stmt-expr (редкий). | Plan 145.1 §11 | P3 |
| ~~`[M-codegen-emission-nondeterminism]`~~ | ✅ **ПОЛНОСТЬЮ ЗАКРЫТА (2026-07-20, worktree `nova-detcodegen`, ветка `p-deterministic-codegen`).** Триггер-часть закрыта Plan 145.2 (2026-06-15, `method_overloads`+`embed_fields` → `BTreeMap`). Остаток (fwd-typedef order + sum-eq conjunct order) закрыт сегодня: аудит всех `HashMap`/`HashSet`-итераций, влияющих на порядок эмиссии, в `emit_c.rs` нашёл 2 живых сайта — (1) `external_names`/`vtable_names` (`HashSet<String>`, район `emit_module` L5570-5683) итерировались напрямую для fwd-typedef `Nova_X`/`NovaVtable_X` — Rust's per-process-random `RandomState`-хешер даёт разный порядок на КАЖДЫЙ `nova build`; (2) `structural_eq_body_for_ptr` (район L18632) итерировал `variants: HashMap<String,Vec<String>>` (клон `sum_schemas`) для `&&`-цепочки sum-eq конъюнктов — тот же эффект. Фикс — `Vec` + `.sort()` по имени (стабильный, осмысленный ключ = C-символ) перед итерацией в обоих местах; чистый tie-break, topo-порядок typedef'ов (unified tuple+fixarr DAG) не тронут. **Доказательство:** 3 цели (пустышка `hello.nv`, collections-driver, флагман `aggregator --strict-effects`) собраны 3× подряд каждая (`NOVA_CACHE=0`) — ДО фикса флагман давал diff 68-86 строк между запусками (переставленные `Nova_F/S/W/D/...` + sum-eq конъюнкты в `ErrSource`/`TlsError`/`JsonValue`/etc.), ПОСЛЕ — **SHA256 идентичен 3/3 на всех трёх целях**. `NovaOpt_typedefs_buf`-остаток из Plan 145.2 §6 (2026-06-15) эмпирически НЕ подтверждён на текущем main (0 совпадений `NovaOpt` ни в одном diff, включая цель, интенсивно гоняющую `Option` через `HashMap.get`/`Queue.pop`/`Deque.pop_back`) — считаю снятым по опровержению, вероятно попутно закрыт недавним `[M-tuple-fixarr-typedef-order]`-фиксом (2026-07-19). Регресс: conformance PASS 503/FAIL 1 (единственный FAIL — известный pre-existing `[M-208-vec-chained-debug-display-red]`, не связан)/SKIP 14; `std/src/checksums` PASS 3/0; `std/src/collections` PASS 13/0; флагман `--strict-effects` собран 6× без ошибок. Детали — [docs/plans/wip/deterministic-codegen-notes.md](wip/deterministic-codegen-notes.md). Не language-changing (внутренний порядок эмиссии), D-амендмент не требуется. Модель: sonnet. | Plan 145.2 §6 + emit_c.rs | ✅ DONE |
| `[M-fiber-arena-sigsegv-install-race]` | ✅ **CLOSED 2026-07-16 (sonnet, ветка `fix-fiber-sigsegv-race`, worktree `nova-fiberfix`, коммит `579691aef`).** Голый check-then-set по `static bool _sigsegv_installed` в `_arena_install_sigsegv_handler` заменён на `pthread_once` (`static pthread_once_t _sigsegv_once`) — тот же идиом, что уже используется в этом файле для `_arena_key_once`, и паритетен с Windows-стороной (`fiber_arena_win.c` уже использовал `INIT_ONCE`/`InitOnceExecuteOnce` для ровно того же process-global one-time install — POSIX-файл был единственным аутлайером). Верификация: (1) Windows — `cargo build --release` (compiler-codegen + nova-cli) чисто; targeted `nova test std/src/concurrency/supervisor_test.nv` (spawn+supervised fan-out) PASS 1/0. (2) Linux/TSan (гейт фикса) — повторён ручной TSan-смоук (чекпоинт волны удалён при закрытии, см. git-историю) (тот же `mn_smoke.c` + `nova_rt/*.c` пересобраны `clang -fsanitize=thread` в WSL2, `~/nova-work`): race по `fiber_arena.c:248`/`:255`/`_sigsegv_installed` **ИСЧЕЗ** из TSan-вывода; race `runq.h:131`/`:273` (init↔grab, НЕ этот маркер — Plan 211 §5 п.4) **остался виден** (подтверждает, что TSan реально работал, а не молчал). Побочно в этом прогоне TSan поймал ещё 2 race вне мандата этой волны (`runtime.c:615` `_sysmon_main` vs `:1082` `_worker_main`; `alloc_boehm.c:110` `nova_alloc`/`_alloc_count`) — не трогал, заметка для будущего M:N-race-аудита (не заведён отдельным маркером этой волной). | floating (runtime, дешёвый фикс) | ✅ done |
| `[M-tsan-race-detector]` | M:N runtime C под `clang -fsanitize=thread` (Go `-race`) → авто-ловит M:N-гонки. **⚠ Windows clang НЕ поддерживает TSAN** (`unsupported for x86_64-pc-windows-msvc` — LLVM limitation, TSAN=Linux/macOS; проверено 2026-06-11). **Prerequisite `[M-nova-linux-build]` ЗАКРЫТ 2026-07-16** — см. [docs/linux-build.md](../linux-build.md): Linux-сборка (cargo+libuv+Boehm+runtime C) верифицирована на WSL2 Ubuntu, `nova build`/`nova test` работают. Ручной TSan-смоук (`clang -fsanitize=thread` на generated `.c` + `nova_rt/*.c` + `libuv.a`, вне обычного CLI-пайплайна) уже сделан на минимальном spawn+supervised примере: компилируется/линкуется чисто, находит 2 реальных data race (`fiber_arena.c` `_sigsegv_installed` install-once check-then-set; `runq.h` init/grab visibility gap — заметки для Плана 211). **Остаётся:** design `--tsan` flag на Linux clang-ветке `test_runner.rs` (сейчас ручной обход через прямой `clang`-вызов) + Boehm-suppressions при необходимости (не потребовались для smoke, но heavier stress tests могут упереться — см. `docker/README.md` Plan 40 находку про pthread-stress) + Linux-CI wiring. | floating (Linux-CI) | P1 |
| `[M-146-growable-stacks]` | Растущие fiber-стеки — снять потолок ~16k одновременных fiber'ов (Plan 82 fixed-8MB). segmented (Boehm-ok, hot-split) vs copying (gated на Plan 144). **ВЕРДИКТ 2026-06-12: ОТЛОЖЕНО** (потребность не доказана; whole-program shadow-stack дорог; см. Plan 146 §6). При упоре в потолок — сперва `[M-fiber-arena-raise-cap]`, не растущие стеки. **УТОЧНЕНИЕ 2026-06-14 (research Q7-Q15, [Plan 144 §7.6](144-precise-gc-implementation.md)):** когда дойдём — путь **segmented** (не двигает стек → не нужен moving-GC, совместим даже с Boehm). **Copying заблокирован дважды:** (а) moving-вердикт §7.6 (general moving в compile-to-C не строим), (б) H5 — замыкания захватывают по указателю В кадры (`T* cap=&local`) → копирование стека инвалидирует их, пока не перейдём на by-value capture. | Plan 146 | P3 |
| `[M-fiber-arena-raise-cap]` | ✅ **SUPERSEDED + IMPLEMENTED — [Plan 149](149-configurable-fiber-arena.md)** (Ф.0-Ф.6 closed+merged 2026-06-12, D233) — configurable fiber arena: стек/макс через env (`NOVA_FIBER_STACK`/`NOVA_MAX_FIBERS`) + nova.toml `[runtime]`, default 8MB→4MB, авто-округление вверх + clamp + garbage→warn+default; compile-time bitmap MAX (262144) отделён от runtime default; per-fiber minicoro stack scales с runtime slot_size. **ЗАМЕТКА о дальнейшем подъёме (2026-06-14):** кап 256k — НЕ ограничение VA (резерв виртуальной памяти почти бесплатен, коммит ленивый). Поднять тривиально: **static bitmap MAX + бронь заранее** (bitmap на 1M слотов = 128 KB; 1M×1MB-резерв = 1 TB из 128 TB → миллионы доступны без динамики; ограничение `max_slots×reserve ≤ 128 TB`). **Динамический chunked-grow** (добронировать чанк по требованию, never-realloc как `NovaSchedState` против grow-vs-wake) — опция, но ИЗБЫТОЧНА (раз резерв бесплатен — проще большой static-кап); схлопывание = decommit ФИЗИКИ свободных слотов (`[M-fiber-stack-lazy-decommit]`), virtual не un-reserve'им. Реальные стены — RAM (N×глубина) и GC-скан (∝ живых стеков), не VA/кап. **Калибровка:** 256k покрывает практически все реальные нагрузки (типичные Go-программы < сотен тысяч горутин; кто >~1M — уходят на пулы/event-driven из-за RAM/GC, не рантайма — кейс Kamardin «A Million WebSockets»). | Plan 149 | ✅ done |
| `[M-fiber-stack-lazy-decommit]` | Возврат **физической** RAM при усадке fiber-стека: декоммит страниц выше high-water mark (Win `VirtualFree(MEM_DECOMMIT)`/`MEM_RESET`; Linux `madvise(MADV_DONTNEED/FREE)`). Виртуальный адрес остаётся зарезервирован → **указатели целы** (адреса те же, физ-страницы вернутся page-fault'ом при регросте). **Делать ЛЕНИВО с ГИСТЕРЕЗИСОМ, НЕ на каждом возврате** (per-return декоммит = syscall+TLB+page-fault thrashing на колеблющихся стеках — тот же hot-split, что у сегментного). **Политика (автор 2026-06-14):** стек растёт вниз — `[SP, base)` занято, `[low_committed, SP)` committed-but-unused; когда занято стало **≤ ¼** committed → декоммитить **нижнюю ½** от `[low_committed, SP)` (отдаём половину, оставляем headroom → мелкие колебания не рефолтят). **Триггер — park fiber'а** (заснул → стек точно не нужен), либо sysmon/GC. **Дополнение, НЕ замена сегментному:** отдаёт RAM простаивающих/усохших fiber'ов, но **НЕ снимает потолок** одновременных fiber'ов (тот из-за ВИРТУАЛЬНОЙ брони 8MB×N, не физической — для потолка нужен меньший резерв/segmented). **ПОЗИЦИЯ автора:** умеренный резерв (1–4MB) + lazy commit + decommit-с-гистерезисом покрывает RAM-гигиену **БЕЗ сегментного**; сегментный — только при доказанной нужде в 100k+ fiber'ов с глубокими стеками. Прецедент: heap-scavenger Go. | Plan 149 / 146 | P3 |
| `[M-comparison-bool-operand-or-chaining]` | `0 <= i < @len` парсится как `(0<=i) < @len` = `bool < @len` → молча **вакуумно-истинно** (range-check обходится; SECURITY для контрактов — `requires 0<=i<@len` вакуумен). Nova сейчас хуже всех peers (даже untyped JS коэрсит; Nova нейтрализует предикат). Решение автора (2026-06-13): **hard-error (как Rust); chained comparison ОТКЛОНЁН** (`&&` явно) → **[Plan 150](150-chained-comparison-relational-safety.md)** ✅ **CLOSED 2026-06-13** (Ф.0-Ф.1: D248 + `E_CMP_CHAIN_UNSUPPORTED` parser + `E_RELATIONAL_OPERAND_NOT_ORDERED` checker; full check-sweep 2938 файлов = 0 регрессий; 13 фикстур plan150). Резолвил Q35; разблокировал `[M-140-bounds-as-contract]`. | Plan 150 | ✅ DONE |
| ~~`[M-d78-duplicate-decl-module-swallow]`~~ | ✅ **CLOSED 2026-07-13 (Plan 202 Ф.1+Ф.1b).** Реестр модулей (резолвер `visited`/`in_progress` + `ModuleSigTable` sig pre-pass) теперь керится по **canonical filesystem path** (`imports::canonical_module_key`), не по декларации — дубль decl из двух физически разных модулей (`a/neg/x.nv`+`b/neg/x.nv`, оба принуждены D29 rev-3 к `module neg.x`) больше НЕ глотает экспорты второго. Синхронно расширен D307/D381 mangling-слой (`emit_c.rs`, `effective_modpath`/`phys_key_of`/`decl_phys_groups`) — иначе C-codegen дал бы redefinition для той же пары (обнаружено ЖИВЫМ CC-FAIL на pos-фикстуре при первом прогоне, не гипотетически). Fixtures: `spec_tests/conformance/d78_dup_decl_registry/` (fn-ось) + `d78_dup_decl_type_axis/` (type-ось) — оба PASS, значения не смешаны. Спека: D78 rev-4 (`spec/decisions/07-modules.md`, keying-семантика амендмент к D29 п.4). Остаточный узкий пробел (НЕ блокирует закрытие — вне acceptance Ф.1b) → `[M-d78-dup-decl-type-cross-import-ambiguous]` ниже. rev-3.1 (`internal/`) ретракция — Ф.4, отдельный followup, НЕ выполнена в этом слиянии. | Plan 202 | ✅ DONE |
| ~~`[M-d78-dup-decl-type-cross-import-ambiguous]`~~ | ✅ **ГОТОВ 2026-07-21 (sonnet, ветка `p-fix-d78-dup`, worktree `nova-d78dup`, НЕ влито — интегратор заберёт).** Root cause подтверждён: D381 branch (2) (`file_type_module`, `emit_c.rs`) дизамбигьюирует селективно импортированный колидирующий тип суффикс-матчем ЗАЯВЛЕННОГО modpath (`cand`) против `imp.path`. Plan 202 Ф.1b добавила `dupN`-тег к `cand` для двух ФИЗИЧЕСКИ различных модулей с идентичной 2-сегментной декларацией — но реальный import-путь (`imports::resolve_module_paths` трактует `parts` как относительный `PathBuf` от package root) НИКОГДА не содержит `dupN`, поэтому суффикс-матч не совпадает НИ С ОДНИМ `dupN`-кандидатом (не «ambiguous», а буквально «no hit» для обоих). Итог: colliding-тип, селективно импортированный ИЗВНЕ объявляющего модуля, деградировал к неквалифицированному `Nova_<Name>` для ОБЕИХ физических сторон → `CC-FAIL` (`undeclared NOVA_TAG_*`). **Фикс:** новая карта `type_def_files` (name, cand) → физический якорь ОБЪЯВЛЯЮЩЕГО peer-файла (`phys_key_of`, тот же canonical dir/file-ключ, что уже строит fn/type dup-axis) + branch (2) fallback `phys_matches` — сравнивает физический путь файла-владельца типа С КОМПОНЕНТАМИ реального import-пути (ровно как это делает сам loader), консультируется ТОЛЬКО когда declared-name суффикс-матч не находит совпадения (весь до-Ф.1b корпус byte-identical). **Фикстура** (репро RED→GREEN): `spec_tests/conformance/d78_dup_decl_type_cross_import/` (`client_a.nv`/`client_b.nv` селективно импортируют `Kind` BARE из `a/neg/kind.nv`/`b/neg/kind.nv` — двух физически разных модулей с идентичной `module neg.kind`); до фикса `CC-FAIL "undeclared identifier NOVA_TAG_Kind_Alpha"`, после — PASS. **Гейты:** d78_dup_decl_registry + d78_dup_decl_type_axis + d78_root_peers + новая фикстура — 4/0; `oot_ancestor_manifest_module_path.rs` cargo test 2/2 ok; `examples/flagship/aggregator --strict-effects` built; полный `spec_tests/conformance` — PASS 130 FAIL 1 (mega-CU `a_q3_println_debug_record`, `E_MATCH_ARM_WIDTH_MISMATCH` на `d407_enum_payload_width.nv` — НЕ регрессия этого слияния, `types/mod.rs` не тронут, уже закрыто в невлитой ветке `p-fix-match-arm-width`) SKIP 22. | Plan 202 (residual) | ✅ ГОТОВ |
| `[M-187-http-serde-setcookie-serialize-collision]` | ✅ **РЕШЕНО 2026-07-16 (sonnet, ветка `fix-serde-dispatch`, worktree `nova-serdefix`).** Root-cause — НЕ dispatch-баг в `emit_c.rs` (не та же семья, что 196.7/98e3663cc, хоть симптом похож): `nova build` (`cmd_build`, `nova-cli/src/main.rs`) **никогда не вызывал** `auto_derive::inject_synthesized_methods_filtered` для `#impl(Serialize)` — в отличие от `nova test`'а (`test_runner.rs`) и `nova-codegen`'s `cmd_compile`, которые это делают. Чекер валидирует `v.serialize(s)` через on-demand `AutoDeriveQueryBridge` (не мутирует `module.items`) → type-check проходит, но codegen не находит НИКАКОГО `FnDecl` под ключом `(RecordType, "serialize")` → вызов внутри mono'нного `json_encode[T]` проходит все receiver-typed dispatch-окна впустую и падает в единственный оставшийся name-only `method_receivers` last-wins fallback → подбирает ЛЮБОЙ другой `@serialize`, зарегистрированный последним в CU (`http`'s `SetCookie @serialize() -> str` — arity/type-несовместимый, отсюда link-error). Фикс: добавлен тот же вызов инъекции в `cmd_build` (перед alpha-rename, зеркалируя `test_runner.rs`) — `emit_c.rs` НЕ тронут, существующие type-directed dispatch-окна уже резолвят корректно, как только FnDecl реально зарегистрирован. Подтверждено минимальной фикстурой (Dto#impl(Serialize)+FooCookie-коллизия, один CU) И реальным репро (examples/flagship/aggregator, http+tls диамант через `nova.local.toml`) — собрано СВОИМ компилятором (`nova build --strict-effects`), curl-smoke `/api/snapshot`+`/api/run`+`/api/events` — корректный typed JSON. Обход в `main.nv` (hand-written `snapshot_dto_json`/`status_dto_json`/`result_dto_json`/`handlers_dto_json`) СНЯТ — `snapshot_body`/`events_body` теперь на `snapshot_to_json` (typed, `report_json.nv`). `emit_record_json`/`EmitRecord` (SSE per-event) сознательно остался hand-written — wire-shape решение (условно опускает `"error"`), НЕ баг-обход, follow-up отдельно. tls/echo_server+echo_client (тот же `nova build`-путь) не регрессировали (TLS handshake+echo OK). Коммиты (ветка `fix-serde-dispatch`, worktree `nova-serdefix`, НЕ влита — интегратор): `a095b961d` (nova-cli фикс) + `5f80b7b1b` (main.nv-снятие обхода). | compiler-codegen (nova-cli build pipeline) | ✅ DONE |
| `[M-match-arm-mixed-int-width-sentinel-coerce]` | ✅ **ЗАКРЫТ 2026-07-21 (worktree `nova-armwidth`, ветка `p-fix-match-arm-width`, sonnet).** Root cause подтверждён репродюсом: `infer_match_common_primitive` (`compiler-codegen/src/types/mod.rs`) бейлило в `None` при ЛЮБОМ расхождении арм-типов (не только genuine mismatch, но и безобидный safe-widening микс), а codegen'ный legacy-фоллбек (`infer_expr_c_type`'s Match-арм / `emit_match`'s собственный arm-type-цикл, ОБА независимые re-derive той же логики) подбирал тип ПЕРВОГО non-`nova_int` арма произвольно — `ro r = match o { Some(v) => v, None => -1 }` (`v u32`) вне аннотированного/return-контекста читало `-1` как `4294967295`. Аннотированный `ro r int = ...` и прямой `-> int` return уже работали (ширина бралась из аннотации/return-типа) — баг проявлялся именно на НЕаннотированном локале, ровно как компенсация после миграции `compose_pair → Option[u32]` убрала исходный unicode-триггер. **Фикс, два слоя** (`types/mod.rs`): (1) `infer_match_common_primitive` — при расхождении арм-типов теперь ПРОВЕРЯЕТ safe-widening (D54 `would_narrow_into`, тот же критерий что уже разрешает неявный `u32→int`-присвоение) и unify'ит к БОЛЕЕ ШИРОКОЙ стороне вместо bail; (2) новая `check_match_arm_width_mismatch` — при GENUINE несовместимости (ни одна сторона не расширяется, напр. `i32` vs `u32` той же ширины — signed→unsigned никогда неявно) эмитит hard `[E_MATCH_ARM_WIDTH_MISMATCH]` ДО кодогена (было: тихий bail → тот же произвольный-arm-баг). Обе построены над общим `match_arm_value_types` (per-arm `(Span, ResolvedType)`, §0/§3 — один источник для канала и диагностики). Language-changing (R2 — новое отклонение ранее молча принимавшихся программ) → **D433** (`spec/decisions/02-types.md`, amends D54/D129/D327) в том же слиянии. Фикстуры: `detect172/u172_2_match_arm_width_pos.nv` + `detect172/neg/n_match_arm_width_mismatch.nv` + `spec_tests/conformance/d129_match_arm_width_widen.nv` + `spec_tests/conformance/neg/n_match_arm_width_mismatch.nv`. Гейты: репро RED→GREEN (6 точечных сценариев); полный `spec_tests/conformance` мега-CU (1004 файла) — **PASS: 518 FAIL: 0 SKIP: 19**; флагман `examples/flagship/aggregator --strict-effects` собран чисто. Коммит(ы) в `p-fix-match-arm-width` (не смёржены в main — интегратор заберёт). | Plan 172.1 (type-engine) / floating | ✅ DONE |

## P4 — Future / opt-кандидаты (без сроков; оживают по триггеру)

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-nonescaping-view-header-stack-alloc]` | **КАНДИДАТ 2026-07-20 (вопрос владельца про `@bytes()` в `@lines`).** Дом-идиом string-модуля `ro bytes = @bytes()` (16 сайтов в 6 файлах) платит 24-байтный GC-заголовок Vec на вызов метода (данные zero-copy, заголовок — куча). View — чистая функция полей `str`; неубегающий заголовок можно класть НА СТЕК (escape-analysis). НЕ делать сейчас: требует opt-слоя над IR (rustc-эталон: MIR-опты; наш AST-only — задокументированный компромисс). **Триггеры оживления:** (1) появление IR/опт-слоя — тогда один из первых дешёвых оптов; (2) профилировка покажет view-заголовки в топе аллокаций; (3) GC-pressure-волны 187-семьи упрутся в header-churn (родня `[M-boehm-large-buffer-retention-fiber-reuse]`). До триггера — НЕ очередь; переписывать 16 методов на unsafe ради 24 байт — запрещённый размен (безопасность > микро-аллокация). | будущий opt-слой / 172-семья | **P4** |

## P2 — Correctness / Completeness

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-newtype-over-fn-type-unsupported]` | **✅ РЕШЕНО 2026-07-23 (ОКНО-5, worktree `nova-okno5`, ветка `p-okno5-fntypes`, sonnet).** Все ТРИ куска реализованы. (1) **Парсинг**: корень был в `parse_type_decl`'s empty-sum `is_body_end` эвристике (`compiler-codegen/src/parser/mod.rs`) — голый `KwFn` СРАЗУ ПОСЛЕ `type X` безусловно трактовался как «пустой sum, `fn` — следующая top-level декларация»; настоящий top-level `fn` ВСЕГДА несёт identifier сразу после keyword (`fn name(`), fn-TYPE body — наоборот, `fn(` без имени; фикс — однотокенный lookahead (`peek_at(1)`): `KwFn` = конец декларации ТОЛЬКО когда следующий токен НЕ `(`. (2) **Авто-подъём `fn → Handler`**: НОЛЬ кода не потребовалось — `single_wrap_candidates`/`assignable` (D55, Plan 200) уже были обобщены на произвольный `Newtype(inner)` независимо от того, скалярный `inner` или `Func`; зафиксировано явным D55-амендментом (не как случайный побочный эффект). (3) **Call-through**: `resolve_fn_typeref` (codegen, `fn_newtype_sigs` pre-scan в `emit_c.rs`) + `fn_type_names` (checker, `BoundCtx` pre-scan в `types/mod.rs`) — значение newtype-над-fn-типом резолвится к своему underlying Func-виду ДЛЯ ЦЕЛЕЙ существующего `fn_param_sigs`/`NovaClosBase` call-dispatch механизма (тот же путь, что и голый fn-параметр — ноль нового codegen). Спека: D52-амендмент + D55-амендмент (`spec/decisions/02-types.md`). Регресс: `spec_tests/conformance/d52_newtype_fn_type.nv` (4 куска, standalone PASS) + neg `d52_newtype_fn_reverse_coerce_neg.nv` (обратная коэрсия — E7301) + neg `d55_fn_newtype_ambiguous_lift_neg.nv` (EXPECT_CC_ERROR — честная диагностика, но на C-уровне, не Nova-checker). **Найдено, НЕ в объёме этого фикса** (новые floating-маркеры ниже): `[M-fn-type-expected-any-bypass]`, `[M-fn-newtype-overload-ambiguity-not-checker-caught]`. **Потребитель:** [222.3 §5а](222.3-extractors.md) — снос `handler_fn` (22 сайта + 14 тест-файлов) — САМА миграция НЕ сделана этой волной (явно вне объёма по заданию), разблокирована. | D52/D55 / парсер+чекер+codegen | ✅ РЕШЕНО |
| `[M-alias-of-fn-type-not-callable]` | **✅ РЕШЕНО 2026-07-23 (ОКНО-5, worktree `nova-okno5`, ветка `p-okno5-fntypes`, sonnet).** Корень: `check_call_callee_not_local_shadow`'s `is_callable_local_ty` (`compiler-codegen/src/types/mod.rs`) смотрело ТОЛЬКО на `TypeRef::Func`/`Pointer`/`Readonly`/`Mut`/`Ref`-обёртки, не разворачивая Named-алиасы вовсе. Фикс: `BoundCtx::fn_type_names` (новый pre-scan поле — имя newtype-над-fn/alias-цепочки-до-fn → underlying `Func` TypeRef, транзитивно для alias, один уровень для newtype) + `is_callable_local_ty` расширен на `TypeRef::Named` веткой через этот набор. Регресс: `d52_newtype_fn_type.nv` кусок 4 (`D52HandlerAlias alias fn(int) -> str`, вызов `next(x)` напрямую — PASS, было `[E_CALL_NOT_CALLABLE]`). | D52 alias / резолв вызова | ✅ РЕШЕНО |
| `[M-fn-type-expected-any-bypass]` | **OPEN 2026-07-23 (найдено ОКНОМ-5 при пробе neg-фикстуры «Handler → fn запрещён»).** `assignable_direct` (`compiler-codegen/src/types/mod.rs`) резолвит `resolved_cat_of(expected)` для ЛЮБОГО fn-типизированного `expected` (`fn(A) -> B`, ЛЮБОЙ сигнатуры) в `ResolvedType::Any` → ранний `return Compat::Ok`, ПОЛНОСТЬЮ пропуская структурную проверку. Симптом: `fn accepts(f fn(int) -> str)`, вызванный с `wrong_sig(x bool) -> int` — ПРИНИМАЕТСЯ чекером без единой ошибки (репро 5 строк, ZERO newtype involvement — не связано с ОКНО-5 работой, найдено ПОПУТНО). Не давал закрыть чистую Nova-level neg-фикстуру «`Handler` (newtype над fn) → голый `fn(...)` параметр — запрещено»: направление УЖЕ корректно нереализуемо (нет обратного unwrap-механизма нигде в дизайне single_wrap_candidates), но локальная демонстрация через call-arg к fn-типизированному параметру никогда не споткнётся — ЛЮБОЕ значение проходит. Cross-newtype (два РАЗНЫХ newtype над одним fn-типом, не взаимоподставимых) корректно ловится через `E7301`, потому что ТАМ `expected` — Named, не голый `Func` literal (не задет этим bypass'ом). Фикс — за пределами 4 куска этого окна: нужна структурная verification fn-сигнатур в `assignable_direct` (params+return поэлементно), которой в чекере попросту никогда не было ни для одного класса fn-значений. | чекер (`assignable_direct`, `resolved_cat_of`) | **P2** |
| `[M-manual-coalesce-corpus-remainder]` | **OPEN 2026-07-24 (найдено волной coalesce-return-retraction — sonnet, worktree `nova-coalesce`/`nova-http-coal`, ветки `p-coalesce`/`p-coalesce-http`).** `W_MANUAL_COALESCE` (реализован, см. закрытые маркеры этой волны ниже в simplifications.md) прогнан по полному корпусу — фактические числа **сильно выше** брифа-инвентаря 2026-07-23 (69 всего): **std/src 66** (было 84 до частичной миграции), **nova-http/src 55** (было 71), **examples 0** (все 6 мигрированы). Мигрированы (эта волна) только сайты, ИМЕННО названные в брифе: `nova-http/src/client/wire.nv` (14), `nova-http/src/middleware/cors.nv:238` (1), `std/src/fs/readfs.nv:113,114`, `std/src/fs/fs.nv:544,624,638`, `std/src/time/civil/tz.nv:33,37,46,128,129,134`, `std/src/time/civil/format.nv:155,165,243,247,252`, `examples/flagship/aggregator` (6) — итого 37 сайтов. Остаток (66 std + 55 nova-http = 121 сайт) не мигрирован — объём оказался ~2.3× брифа-оценки (69→161 фактических находок до миграции), полная миграция вне бюджета этой волны. Прогон: `nova lint --rule W_MANUAL_COALESCE <path>` даёт точный список с файл:строка. | миграция остатка (std/src ~66, nova-http/src ~55) | **P2** |
| `[M-assoc-const-out-of-body-syntax]` | ✅ **ЗАКРЫТО 2026-07-24 (sonnet, worktree `nova-oobconst`, ветка `p-oob-assoc-const`, окно №66).** Парсер (`parser::parse_const_decl`, `compiler-codegen/src/parser/mod.rs`) принимает `Type.NAME` как qualified module-level const name (первый ident + `.` + второй ident → `"Type.NAME"` в `ConstDecl.name`; T-dependent `Box[int].SIZE` не распознаётся — followup). Резолв — новая `imports::attach_out_of_body_assoc_consts` (`compiler-codegen/src/imports.rs`), вызывается ОДИН раз в `resolve_imports_inline_ex` сразу после финального flatten `module.items` (единая точка для всех трёх канонических pipeline'ов — module doc-comment `imports.rs`): вырезает qualified `Item::Const` из плоского списка, переносит как `AssocConst` в `TypeDecl.assoc_consts` найденного по имени типа — тот же const-table путь, что и in-body форма (namespace-доступ, `E_CONST_INSTANCE_ACCESS`, `.rodata` emission в `emit_c.rs` — ВООБЩЕ без изменений, читают `assoc_consts` как раньше). Cross-peer-file (тип в одном файле folder-module, const — в другом) работает, т.к. merge идёт над уже-flatten'ённым списком. **In-body форма отвергнута целиком** (не deprecated — «одна дверь» реализована на парсер-уровне): `parse_assoc_const_field` удалена, обе ветки (`const`/`export const` внутри `type X { … }`) — hard error `[E_CONST_IN_BODY_RETRACTED]` с migration-hint. **Комбо с №60 подтверждено:** `StatusCode.OK`/`Rect.UNIT` (составное значение) работают через out-of-body форму без единого изменения в codegen. **Фикстуры:** `spec_tests/conformance/d200_associated_const.nv` мигрирован целиком на out-of-body (δ0 — те же 7 test-блоков/assert'ы + добавлен namespace-only regression test), PASS; новые neg `nova_tests/plan114/neg/d200_oob_instance_access_neg.nv` (`E_CONST_INSTANCE_ACCESS` на out-of-body decl), `d200_in_body_const_retracted_neg.nv` (`E_CONST_IN_BODY_RETRACTED`); мигрированы (та же семантика/код ошибки, другой синтаксис) `plan114_4_1_literal_includes_assoc_neg.nv`, `plan114_4_1_instance_access_neg.nv`, `d200_composite_ctor_call_neg.nv`, `d200_composite_str_field_neg.nv`. **Регресс:** `nova_tests/plan114/neg/*.nv` (37 файлов) — 36 PASS, 1 FAIL (`plan114_4_4_trampoline_generic_no_context_neg`) — **тот же pre-existing FAIL**, задокументированный окном №60 (несвязанный `E_READONLY_COERCE` regression, вне мандата этого окна). Грепом подтверждено: std/nova-http/examples не используют in-body форму (0 сайтов) — миграция реального кода не требовалась. D200-амендмент дописан (`spec/decisions/02-types.md`) — финализация парсера + композитный пример переписан out-of-body (был in-body в тексте §финализации №60, до этого окна). Полный `spec_tests/conformance` мега-CU и flagship не гонялись (CPU-дисциплина, targeted only) — авторитетный гейт передан владельцу. | парсер + резолв (D200 out-of-body) | ✅ ЗАКРЫТО |
| `[M-d200-assoc-const-composite-value]` | ✅ **ЗАКРЫТО 2026-07-24 (sonnet, worktree `nova-n60`, ветка `p-fix-n60-assoc-const`, окно №60).** Корень: `emit_const_expr`/`emit_const_expr_typed` (`emit_c.rs`) не имели ветки для `ExprKind::RecordLit` — падали на generic fallback «non-constant expression». Фикс: новый `emit_const_record_lit` строит top-level designated-initializer (`{ .field = <value>, ... }`) поверх `record_schemas`-схемы типа, рекурсивно (вложенные записи — тот же путь); reference на другой const в поле резолвится тем же `Ident`-веткой, что и скаляр. **Побочный фикс (обязательный, не опциональный):** assoc-const emission-loop в `emit_type_decl` был эмитен ДО struct/value-record body своего же типа — self-referential составное значение (`StatusCode.OK` внутри `type StatusCode`) требует typedef СНАЧАЛА → loop перенесён ПОСЛЕ `match &t.kind {..}` (byte-identical для скаляров, ничего не зависело от порядка). **Второй побочный фикс:** `Type.NAME.field` (доступ к полю СОСТАВНОГО assoc-const, `StatusCode.OK.code`) парсится в 3+-сегментный `ExprKind::Path`, emit_expr которого имел ветку ТОЛЬКО на `parts.len()==2` — добавлена ветка `parts.len() > 2`, синтезирующая `Ident(symbol)+Member-chain` (зеркалит соседнюю local-var-shadow ветку). **Границы (по заданию, не расширено):** (1) `str`-поле внутри записи → честный отказ с диагностикой `[M-d200-assoc-const-composite-value]` (nova_str на file-scope нуждается в отдельном `.rodata`-буфере байт, не строится этим путём; скалярный `str`-const вне записи — работает как раньше, отдельная ветка); (2) конструктор-вызов (`= StatusCode.mk(200)`) — остаётся `E_CONST_NOT_CONSTEXPR` (текст явно добавлен в codegen-fallback для явного code вместо generic-сообщения), пин-neg. **Фикстуры (RED→GREEN):** `spec_tests/conformance/d200_associated_const.nv` — 3 новых теста добавлены (namespace+field read, вложенная запись, expr/сравнение), δ0 на 3 существующих (скаляр) подтверждён; `nova_tests/plan114/neg/d200_composite_ctor_call_neg.nv` (E_CONST_NOT_CONSTEXPR), `nova_tests/plan114/neg/d200_composite_str_field_neg.nv` (честный отказ) — оба PASS. **Регресс:** все 34 фикстуры `nova_tests/plan114/neg/*` — targeted `test-build`, PASS кроме `plan114_4_4_trampoline_generic_no_context_neg` — **ПОДТВЕРЖДЕНО pre-existing на немодифицированном HEAD** (изолирующий build без моего диффа воспроизводит тот же `E_READONLY_COERCE` вместо `E_CONST_FN_FIRST_CLASS` — не регрессия этого окна, вероятно побочный эффект недавних `[M-ro-launder-via-mut-binding]` слияний, вне мандата). std/examples не задеты (грепом подтверждено: in-body assoc-const синтаксис нигде не used за пределами новых фикстур). D200-амендмент дописан (`spec/decisions/02-types.md`) — снята «only scalar» оговорка, задокументированы границы + codegen-заметка про порядок emission. Полный `spec_tests/conformance` мега-CU и flagship `--strict-effects` НЕ гонялись (CPU-дисциплина, targeted only) — авторитетный гейт передан владельцу. Задача №43 (closure `\|\|Type.new()`) не начата — лимит окна исчерпан на основной задаче. | компилятор (assoc-const codegen, D200; `spec/decisions/02-types.md`) | ✅ ЗАКРЫТО |
| `[M-statuscode-ctor-result-vs-contract]` | **OPEN 2026-07-24 (вопрос владельца, ФИНАЛ по nv-coding-style R1/R3а).** Текущий `StatusCode.new(int) -> Result` навязывает `?`/`!` даже на литералах. Разобрано: `try_new` ОТВЕРГНУТ (R3а: `try_`=bool-попытка/`try_from`-конверсия, НЕ Result-сиблинг `new`); контрактом целиком заменять НЕЛЬЗЯ (`requires`-нарушение panic-класс → сетевой мусор = DoS-паника фибры). **Итоговый API:** (1) `fn StatusCode.new(int) -> Result[StatusCode, HttpError]` — primary валидирующий (R1/R2, как `Date.new`/`TimeOfDay.new`, 6 std-сайтов); (2) `unsafe fn StatusCode.new_unchecked(int) -> StatusCode` — escape hatch, **unsafe обязателен** (std-прецедент `to_str`/`to_str_unchecked`; пропуск проверки чеканит невалидный 999 → ломает инвариант `@class()`; явная greppable дверь vs молчаливая течь newtype Q2); добавлять ПО ФАКТУ горячего пути, не спекулятивно (литералы закрыты константами + `.new(x)!`); (3) консты `StatusCode.OK` — частые литералы ([M-d200/assoc-const]). **Отвергнуты мои прежние имена** `from_u16`/`.of`/`try_new` — не по конвенции. **Часть [222.5 §4б](222.5-respond.md).** | nova-http `status.nv` | **P2** |
| `[M-open-range-len-source-hardcoded]` | **✅ ЗАКРЫТ 2026-07-24 (влит b7ad3157f, мега-CU 556/0/67): структурный len-int предикат вместо свитча str/Vec; регрессия str.from устранена cherry-pick 5435d4fe4 — убрана лишняя глобальная str-регистрация; open-range открыт user-типам с len int; попутно латентный double-eval receiver.** **OPEN 2026-07-24 (наблюдение + правило владельца).** Лоуэринг открытого диапазона `[a..]` в `emit_c.rs` захардкожен свитчем по C-типу: str (~34828) `if obj_ty=="nova_str"` → `_s.len` (прямое поле); Vec (~34745) `Nova_Vec____` → синтез `obj.len()`; `NovaArray_` → своя ветка; `else → Err unsupported`. **Довод интегратора «неизбежный рантайм-диспатч str vs Vec» ОПРОВЕРГНУТ владельцем:** сам срез УЖЕ в .nv — `Vec[T] @index(r Range)` (`collections/vec/slice.nv:42`) и `str @index(r Range)` (`runtime/string/slice.nv:35`); рантайм-функции зовутся из ТЕЛ этих .nv-методов, диспатч идёт штатным резолвом. Хардкод — ТОЛЬКО в материализации открытого end перед передачей в `@index`. **ПРАВИЛО (владелец 2026-07-24):** материализация end смотрит структурно на поле `len int` у типа — есть → берёт из него; нет → ошибка. Проверено: и str (`{ptr *u8, len int}`), и Vec (`priv {… len int …}`) поле имеют → свитч `str vs Vec` схлопывается в одну структурную проверку. **Уточнение:** фикс-массив `[N]T` рантайм-поля `len` НЕ имеет (длина `N` в типе, compile-time) → остаётся отдельной законной type-level веткой (`fa_total`). Итог правила: поле `len int` → читать; иначе фикс-массив → статический N; иначе Err. Побочно: пользовательский тип с полем `len int` + `@index(Range)` получит open-range-срез бесплатно (снимает нынешний `else → Err` для юзер-типов). Меньше моего прежнего предложения (протокол `HasLen` — оверинжиниринг снят). | codegen (`emit_c.rs` материализация open-end → структурная проверка `len int`) | **P2** |
| `[M-method-value-arg-in-generic-combinator-infer]` | **OPEN 2026-07-24 (найдено при оценке сахара `a.map^method()`).** Method-value как аргумент generic-комбинатора не даёт вывести method-level type-param. Репро: `ro a Option[int] = Some(42); a.map(int.@to_str)` → **`cannot infer method-level type argument `U` for `Option.map``**. `int.@to_str` = `fn(int) -> str` (D35 unbound method-value), `map[U](f fn(T)->U) -> Option[U]` должен вывести `U := str` из возврата method-value. Замыкание работает: `a.map(|v| v.to_str())` PASS. **ТОЧНАЯ ЗОНА (сверено 2026-07-24; прежняя запись «types/mod.rs» БЫЛА НЕВЕРНА):** `compiler-codegen/src/codegen/emit_c.rs`, функция `resolve_method_level_subst` (~23488). Там **Step 1** (~23541) связывает type-param из НЕ-замыканий (`infer_type_param_binding(param.ty, arg_c, subst_slots)`), **Step 2** (~23573) — из замыканий (пред-вывод return-типа ClosureLight/ClosureFull). Method-value-аргумент проваливается сквозь ОБА (не замыкание; `infer_expr_c_type` на нём не даёт `fn(T)->U`-структуру для связывания `U`) → `subst_slots[U]` остаётся None → диагностика на ~23757. **ФИКС:** добавить Step 3 — если аргумент это method-value (`Type.@method`, детект как в `emit_method_value_typed`/чек ~12958), резолвнуть его fn-сигнатуру и `infer_type_param_binding(&param.ty, &method_value_c_sig, &mut subst_slots)` против `fn(T)->U`-структуры параметра (зеркало Step 2, но сигнатура известна статически, не из тела). **Значимость:** «одна дверь» для arg-less подъёма метода над функтором (`a.map(int.@to_str)` вместо сахара `a.map^method()` — глиф `^` занят XOR; `_`/`@` тоже заняты). Arg-bearing (`v.clamp(lo,hi)`) НЕ покрывается (method-value аргументы не связывает — остаётся замыкание). **Фикстуры:** pos — method-value в `map`/`filter`/`flat_map` на разных типах (`int.@to_str`, `str.@byte_len` и т.п.), вывод type-param верный; neg — реально неоднозначный method-value (перегрузка) остаётся честной ошибкой. Гейт: мега-CU (codegen-инференс) + фикстуры. | codegen (`emit_c.rs::resolve_method_level_subst`, +Step 3 method-value) | **P3 (эргономика; замыкание-обход есть)** |
| `[M-generic-body-calls-generic-mono-placeholder]` | ✅ **РЕШЕНО 2026-07-31 = 221.1 №170 (сведение имён; фикс окном p-genmono, влит интегратором).** Исходная запись: **OPEN 2026-07-31 (поднят интегратором из код-комментария 236-исполнителя — вопрос владельца «почему 10 перегрузок, а не fn[T Ints]»).** Generic-функция, вызывающая ДРУГУЮ generic-функцию из СВОЕГО generic-тела, НЕ монообразуется: codegen эмиттирует вызов неразрешённого generic-плейсхолдера, возвращающего исходный скаляр вместо результата (C: «initializing NovaValue_BigInt with an expression of incompatible type 'int'»). **Кейс:** `fn[T Ints] T @to_bigdecimal() => BigDecimal.new(@to_bigint(), 0)` — bounded-вызов `@to_bigint()` (сам `fn[T Ints]`, bigint.nv:405, кросс-модульный) из generic-тела. КОНКРЕТНАЯ функция → generic (`fn i8 @to_bigdecimal() => …@to_bigint()…`) монообразуется нормально — потому fallback работает. **Проверено интегратором 2026-07-31:** схлопывание 10 перегрузок в один generic → CC-FAIL, откат. Минимальный репро — в отчёте плана 236 (bigdecimal Ф.5 «обход двух compiler-дефектов»). **Blast:** вынужденное D9-нарушение — 10 копий одного тела в `bigdecimal.nv` (+ то же ждёт bigfloat 237: `fn[T Ints] T @to_bigfloat` из плана); после фикса схлопнуть перегрузки обратно (комментарий-обоснование в bigdecimal.nv:150-161 снять той же волной). | codegen (мономорфизация: generic-вызов внутри generic-тела, кросс-модульный bounded) | **P2 (D9-долг ×2 пакетов)** |
| `[M-lint-phantom-prelude-unused-import]` | ✅ **РЕШЕНО 2026-07-31 (окно p-lints, 2 корня: транзитивный peer-граф без is_entry_module-фильтра + line:col-рендер без учёта file_id; A/B ровно на 7 именах владельца; WARN std 1079→60).** Исходно: **OPEN 2026-07-31 (линт-проход по nova-bigint по просьбе владельца).** `unused-import` даёт ФАНТОМНЫЕ предупреждения об именах, которых в файле НЕТ: на `nova-bigint/src/bigint.nv` — 7 шт (`Vec` ×3, `HashMap`, `VecIter`, `Set`, `RawMem`), файл ни одно из них не импортирует. Спаны битые: «`Vec` at 9:48» указывает в `import std.prelude.core.{Option, Some, None, Result, Ok, Err}` (Vec отсутствует), «31:9» — в doc-КОММЕНТАРИЙ `/// Целое произвольной точности…`. Источник имён — авто-прелюдия/транзитивные импорты (Vec/HashMap/RawMem — прелюдные), т.е. чекер вешает unused-диагностику ЧУЖИХ (синтезированных) импортов на спаны пользовательского файла. Шум хоронит реальные находки (14 WARN, из них половина — фантомы). Воспроизводится: `nova check --lint src/bigint.nv` (внешняя репа, NOVA_STD_PATH). Упоминался и в REPORT bigint-исполнителя («7× prelude imports»). Фикс: unused-import не должен репортить синтезированные/прелюдные импорты (или должен вешать их на настоящий источник, не на случайные спаны файла). | чекер (unused-import: фильтр синтезированных импортов + спаны) | **P3 (шум линта во внешних репах)** |
| `[M-ro-assoc-const-decl-ice]` | ✅ **РЕШЕНО 2026-07-31 (worktree `nova-p157`, ветка `p157-ro-assoc-const`, sonnet, [Plan 157](221.1-bug-sweep.md) №157).** Реализована форма `ro Type.NAME [Тип] = expr` — associated **ro**-value, non-constexpr (владелец лично предложил, `ro BigInt.ZERO BigInt = {...}` — реализовано и проверено локальной BigInt-класс фикстурой, без похода в nova-bigint). Парсер: `ro Type.NAME` (lookahead `KwRo Ident Dot Ident`) парсится через новую `parse_assoc_ro_decl` → `Item::Const(ConstDecl{is_lazy_ro:true})` — та же qualified-name форма, что `const Type.NAME`, is_export threaded (LetDecl этого не умел). `imports::attach_out_of_body_assoc_consts` переносит `is_lazy_ro` в `AssocConst` без дублирования логики (одна ветка на оба случая). Чекер: namespace-доступ/`E_CONST_INSTANCE_ACCESS`/export — переиспользованы БЕЗ ИЗМЕНЕНИЙ (общий `TypeDecl.assoc_consts`); НОВОЕ — strict const/ro partition-симметрия на assoc-уровне (`E_RO_FOR_CONSTEXPR_PREFER_CONST`, зеркалит module-level `check_ro_module_partition`) и **закрытие пре-существующей дыры** — `Type.NAME = …` (reassignment) не проверялся ВООБЩЕ ни для `const`, ни для `ro` (checker молчал; C-уровень для `ro`-формы реально писабелен, т.к. storage — небезопасный non-const global) — добавлена проверка `E_LOCAL_NOT_MUT` (тот же код, что у reassignment обычного `ro`-локала) для ОБЕИХ форм. Codegen: **переиспользована** существующая машина module-level `ro` (`emit_lazy_const`, Plan 152.4) — символ `Type_NAME` передан и как Nova-ключ, и как C-qualifier; emit в ТОЙ ЖЕ фазе, что bare `ro` (после generic-type-defs — критично для кучевых generic-полей типа `[]u32`); `emit_c.rs` НЕ нарастили новым дублирующим путём (три точки правки: skip в существующем assoc-const-loop, новый loop рядом с bare-`ro`-loop, однa строка в Path-2seg read arm). D200-амендмент — `spec/decisions/02-types.md` (таблица const-vs-ro, codegen, известные смежные разрывы). Фикстуры: `spec_tests/conformance/plan157_ro_assoc_value.nv` (pos, PASS в мега-CU 313.74с) + 2 neg (`plan157_ro_assoc_reassign_neg.nv`/`plan157_ro_assoc_prefer_const_neg.nv`, оба PASS через `nova test`). **Побочно найдено (НЕ пофикшено, отдельные маркеры ниже):** (а) `infer_expr_c_type`'s P67-LEGACY финальный Path-fallback резолвит НЕАННОТИРОВАННЫЙ локал через ГОЛЫЙ последний сегмент имени в ГЛОБАЛЬНОЙ (не type-qualified) таблице — коллизия при совпадении last-segment с ЛЮБЫМ несвязанным bare top-level const/assoc-const где-то в CU (воспроизведено идентично для уже отгруженного `const Type.NAME` — НЕ specific to `ro`); (б) unbound 4-сегментная цепочка `Type.NAME.field.method()` (field-THEN-call, без промежуточного биндинга) — тот же класс ICE, что уже закрытый 3-сегментный `[M-assoc-const-chained-method-call-p67]`, но НЕ покрывающий field-access-затем-call; воспроизведено идентично для `const Type.NAME` — тоже НЕ specific to `ro`. | парсер (`parser::parse_assoc_ro_decl`) + чекер (`types/mod.rs`: `is_lazy_ro`-partition + assign-target check) + codegen (`emit_c.rs`: `emit_lazy_const` reuse) + спека (D200 amend) | ✅ done |
| `[M-assoc-name-bare-lastseg-typeinfer-collision]` | **OPEN 2026-07-31 (найдено при авторстве Plan 157 pos-фикстуры — `ro z = SomeType.ZERO` инферился как `NovaValue_Duration` вместо `NovaValue_SomeType`).** `emit_c.rs`'s `infer_expr_c_type` финальный `[P67-LEGACY]` Path-fallback (~строка 61590) резолвит тип НЕАННОТИРОВАННОГО локал-биндинга (`ro z = Type.NAME`) через `self.var_types.get(<ГОЛЫЙ последний сегмент Path>)` — ГЛОБАЛЬНУЮ (НЕ type-qualified) таблицу. Если ДВА разных типа имеют associated `const`/`ro` (или один — assoc, другой — bare top-level const) с ОДИНАКОВЫМ последним сегментом имени где-то в одном compile unit (конкретный, реально бьющий случай: любой будущий `Type.ZERO` рядом с `std/src/time/duration/core.nv`'s `Duration.ZERO`, транзитивно в КАЖДОМ CU) — инференс типа локала выбирает НЕ ТОТ тип → CC-FAIL member-not-found на несвязанной структуре. Явная типовая аннотация на биндинге (`ro z Type = Type.NAME`) — обходной путь (проверено, работает). Воспроизводится идентично для уже отгруженного `const Type.NAME` — НЕ specific to `ro`-формы (Plan 157). **Фикс:** до "последний сегмент"-фоллбека проверить "joined"-форму (`parts.join("_")`) — которая УЖЕ является следующей веткой ниже, просто идёт ВТОРОЙ; поменять порядок ИЛИ сделать last-segment-lookup type-qualified (искать `(type_name, name)`-пару, не голое имя). | codegen (`emit_c.rs` `infer_expr_c_type` P67-LEGACY Path fallback, ~61590) | P2 (молчаливый CC-FAIL misinfer, найдено Plan 157) |
| `[M-assoc-const-chained-field-then-method-p67]` | **OPEN 2026-07-31 (найдено при авторстве Plan 157 pos-фикстуры — `match Type.NAME.field { ... }` и `Type.NAME.field.method()` без промежуточного биндинга).** Unbound 4-сегментная цепочка (`Type.NAME.field.method()`) НЕ покрыта существующим `assoc_const_chain_as_member`-хелпером (`emit_c.rs`, гейт `p.len() == 3` — только `Type.CONST.method()`, 3 сегмента) → падает в `[P67-LEGACY] Path call return type unknown` ICE (та же паника, что исходно описывал `[M-ro-assoc-const-decl-ice]`, №157 — независимо от неё воспроизводится и для `const Type.NAME`, НЕ specific to `ro`). Смежно: unbound 3-сегментное ПОЛЕ-БЕЗ-вызова (`match Type.NAME.field { ... }`, поле читается корректно через lazy-const-путь, но match-scrutinee temp-var декларируется БЕЗ типа — `use of undeclared identifier '_nv_scr_N'`) — отдельный, но родственный codegen-разрыв в match-scrutinee-инференции для unbound assoc-путей. Обход — биндинг перед использованием (`ro x = Type.NAME; match x.field { ... }` / `x.field.method()`), уже используется в Plan 157 pos-фикстуре. **Фикс:** расширить `assoc_const_chain_as_member`-класс хелперов на field-then-call (4 сегмента) и на match-scrutinee-инференцию для unbound 3-сегментного field-read. | codegen (`emit_c.rs` `assoc_const_chain_as_member` + match-scrutinee type inference) | P2 (ICE + отдельный CC-FAIL, найдено Plan 157) |
| `[M-str-empty-assoc-const-builtin-blocked]` | **OPEN 2026-07-31 (найдено при попытке снять `[M-175-value-record-const-ref]` сайт №17, `runtime/string/core.nv` → `Str.EMPTY`, Plan 157).** D200/Plan 157 out-of-body assoc-const/ro (`const`/`ro Type.NAME`) требует УЖЕ ОБЪЯВЛЕННЫЙ `TypeDecl` в compile unit'е для attach'а (`imports::attach_out_of_body_assoc_consts` матчит `Item::Type(td) if td.name == type_name`) — `str` (и другие built-in примитивы: `int`/`bool`/…) НЕ являются user `TypeDecl` (compiler-intrinsic lang-item, методы на них объявляются через `fn str @method()`/`fn str.static()`, но БЕЗ соответствующего `type str { … }` в дереве) → `const str.EMPTY str = ""` компилируется (парсер принимает), но REF `str.EMPTY` не резолвится (undeclared `str_EMPTY`, проверено пробой). Это блокирует ЛЮБУЮ assoc-const/ro форму на built-in примитивах, не только `Str.EMPTY`. Аудит-сайт `runtime/string/core.nv` (STD_AUDIT №17) НЕ тронут этой волной — `str.new() -> Self => ""` (текущий канон, D200/coding-style-корректный constructor-idiom) оставлен как есть; вводить `Str.EMPTY` без этого фикса невозможно. | чекер/imports (`attach_out_of_body_assoc_consts` — нужен путь для built-in примитивных receiver-типов, не только user `TypeDecl`) | P3 (заблокированная фича, не баг активного кода) |
| `[M-socket-addr-port-only-form]` | **✅ РЕШЕНО 2026-07-30 (worktree `nova-convfix`, ветка `p-convfix`, sonnet).** Реализовано ровно по разбору ниже: (а) `net_addr_parse` (`nova_rt/net.c`) — пустой host перед `:` теперь парсится как `0.0.0.0` (Go/nginx wildcard); (б) `SocketAddr.any(port int)`/`SocketAddr.any_v6(port int)` добавлены (v4 — структурный `net_addr_v4_into(0,0,0,0,...)`, v6 — новая C-функция `net_addr_any_v6_into` на `uv_ip6_addr("::", ...)`), зеркало `loopback`/`loopback_v6`; **поправка владельца 2026-07-30 (в той же волне):** порт всей публичной `SocketAddr`-конструкторской поверхности (`loopback`/`loopback_v6`/`v4`/`any`/`any_v6`) — `int` + `requires port >= 0 && port <= 0xffff` (D24 TrivialBackend, статически доказуемо для литералов), НЕ `u16` — `u16`-граница на входе иллюзорна (`AGGREGATOR_PORT=70000` → `as u16` молча даёт 4464); `u16` остаётся только у самой C-границы (`ffi.nv extern`/`net.c`) и у read-only геттеров (`@port()`, `local_port()`/`peer_port()` — представление, не вход); флагман (`resolve_port`/`resolve_addr`/`aggregate.nv`) мигрирован на `int`, лишние `as u16`-касты убраны; (в) дефолт флагмана (`?? SocketAddr.loopback(port)`) НЕ тронут; (г) миграция формы прошла в той же волне, что `[M-from-str-static-conversion-lint-gap]` (`str @to_socket_addr`). Доккомменты на `@to_socket_addr`/`any`/`any_v6` явно оговаривают «`:port` = все интерфейсы, НЕ loopback». **Сверка README nova-polaris:** строковой перегрузки `serve(app, ":8080")` НЕТ (грепом подтверждено 0 совпадений) — README расходится с реализацией уже сейчас, независимо от этой волны; nova-polaris НЕ чинил (отдельный репозиторий, вне зоны правки этой волны) — открытый вопрос для владельца polaris. Гейты — см. отчёт волны конвфикса (`nova test std/src/net` δ0, флагман `--strict-effects` собран). Разбор, приведший к решению (сохранён как контекст): **OPEN 2026-07-30 (вопрос владельца: «в конфигах loopback выглядит `:port` — может `":${port}".to_socket_addr()`?»).** Разобрано с СЕМАНТИЧЕСКОЙ поправкой: конфиг-форма `:port` общепринята (Go `net.Listen("tcp", ":8080")`, nginx), но означает она **ВСЕ ИНТЕРФЕЙСЫ** (0.0.0.0/INADDR_ANY), НЕ loopback — заменять ею дефолт флагмана `?? SocketAddr.loopback(port)` НЕЛЬЗЯ (смена безопасного localhost-дефолта на открытый наружу; флагман-коммент осознанно разделяет: дефолт loopback, наружу — только явный `AGGREGATOR_BIND=0.0.0.0` для Docker). **Два реальных пробела:** (1) `":8080"` сейчас НЕ парсится вовсе — `net_addr_parse` (`nova_rt/net.c:235`) режет по `:`, пустой host → `uv_ip4_addr("")` → ошибка; при этом **README nova-polaris уже обещает `polaris.serve(app, ":8080")`** — README расходится с реализацией; (2) `SocketAddr.any(port)` ОТСУТСТВУЕТ (есть только `loopback`/`loopback_v6`) — wildcard-bind выразим лишь строкой. **Фикс:** (а) парсер принимает пустой host как wildcard (Go-семантика; ДОКУМЕНТИРОВАТЬ: `:port` = 0.0.0.0, не loopback); (б) добавить `SocketAddr.any(port u16)` (+ v6-парный); (в) дефолт флагмана НЕ трогать; (г) миграция формы — вместе с `[M-from-str-static-conversion-lint-gap]` (`str @to_socket_addr`). | std/net (`addr.nv` + `nova_rt/net.c` parse) + polaris README-сверка | **P3** |
| `[M-from-str-static-conversion-lint-gap]` | **✅ РЕШЕНО 2026-07-30 (worktree `nova-convfix`, ветка `p-convfix`, sonnet).** (1) Линт `W_STATIC_CONVERSION` (`lints.rs:4012`) — добавлено точное имя `from_str` в match (НЕ расширено слепо на `from_*`: инвентарь по std/nova-http/nova-polaris показал, что `from_polar`/`from_bits`(×2)/`from_mode`/`from_os`/`from_code`/`from_raw`(×5)/`from_image`/`from_epoch_day`/`from_normalized`/`from_seconds`/`from_hm`/`from_weeks`/`from_nanos_of_day`/`from_tzif`/`from_number`(×2)/`from_net`/`from_tls`/`from_compress` остаются легальными концепт-источниками §1а — детали в отчёте волны); юнит-тесты pos (`Path.from_str`-форма)/neg (`from_polar`, `from_raw_parts`) добавлены. (2) `std/fs/path.nv`: `str @to_path() -> Path` заменил `Path.from_str` (декла снесена без deprecate-цикла), 7 code-сайтов + 2 doc-сайта мигрированы. (3) `std/net/addr.nv`: `str @to_socket_addr() -> Result[SocketAddr, NetError]` заменил `SocketAddr.from_str` (декла снесена), 5 code-сайтов (вкл. флагман `examples/flagship/aggregator/src/main.nv:140`, дефолт loopback не тронут) + 1 doc-сайт мигрированы. **Побочно найдено, НЕ мигрировано (вне мандата этой волны, новые followup-кандидаты):** `str.from_utf16(units []u16)` — тот же класс, что уже ретрактированный `str.from_bytes` (`[]u16` — значение-ресивер, не концепт); nova-polaris (отдельный репозиторий) — обширная `T.from_request(req)`-семья (11 сайтов), структурно похожая на запрещённый паттерн, вне зоны правки этой волны. Гейты — см. отчёт волны (`cargo test --lib lints::` зелёный, `nova test std/src/fs`+`std/src/net`+`std/src/data` δ0, грепом `from_str`=0 в std/examples). Разбор, приведший к решению (сохранён как контекст): **OPEN 2026-07-30 (найдено при разборе флагман-сниппета `env(...).flat_map(|host| SocketAddr.from_str(...))` — вопрос владельца про идиому в других языках).** Семья `T.from_str(s)` — запрещённая статик-конверсия по §1а nv-coding-style (ретракт 2026-07-09: `T.from(s)`/`T.parse(s)` = «пятая дверь», ломает цепочки), но **линт `W_STATIC_CONVERSION` её НЕ ловит**: детектор матчит ТОЛЬКО буквальные имена (`lints.rs:4012` — `f.name == "from" || f.name == "parse"`), `from_str`/`from_bytes`-морфология проскакивает (проверено: `nova check --lint std/src/fs/path.nv` — молчит на декле). **Инвентарь:** 2 деклы — `Path.from_str(s) -> Path` (`std/fs/path.nv:83`, infallible) и `SocketAddr.from_str(s) -> Result[SocketAddr, NetError]` (`std/net/addr.nv:78`); **17 call-сайтов** (Path 9: fs-тесты; SocketAddr 8: включая `examples/flagship/aggregator/src/main.nv:140`). Канон-сиблингов НЕТ (`str @to_path`/`@to_socket_addr` — греп 0) — не «известный остаток», а недомигрированный хвост §1а-волны. **Фикс:** (1) линт — матчить `from_str` (и проверить `from_bytes`-родню; осторожно: `str.from_utf16`-класс легален — «источник не-ресивер» там не выполняется? нет, []u16 может быть ресивером — сверить §1а-исключения по списку ретрактированных, НЕ расширять слепо на `from_*`); (2) std — добавить `str @to_path() -> Path` + `str @to_socket_addr() -> Result[SocketAddr, NetError]`, `from_str`-деклы ретрактировать; (3) миграция 17 сайтов (флагман: `"${host}:${port}".to_socket_addr().ok()`). Связан: `[M-lint-findings-static-conversion]` (волна §1а, 2026-07-09). | линты (`lints.rs:4012`) + std (`path.nv`, `addr.nv`) + миграция | **P3** |
| `[M-inline-cast-receiver-method-resolution]` | **✅ ЗАКРЫТ (234 часть B шаг 1, adcdc3fa3, 2026-07-29; доверифицирован окном p-int128 2026-08-01 пробой при двух disjoint-blanket'ах). Было: OPEN 2026-07-27 (найдено при реализации `std/math/int128.nv`).** Инлайн-каст в позиции ПРИЁМНИКА не влияет на резолв метода: `(x as u64).m()` выбирает НЕ ту перегрузку, что `ro v = x as u64; v.m()`. **Два наблюдённых следствия (оба измерены):** (1) **тихая бесконечная рекурсия** — `export fn int @to_i128() => (@ as i64).to_i128()` уходит в себя (каст `int→i64` не переключает резолв на `i64`-перегрузку) → **stack overflow в рантайме, без диагностики**; (2) **неверная перегрузка** — `(0xFFFFFFFFFFFFFFFF as u64).to_i128()` идёт мимо `u64 @to_i128` (даёт отрицательное значение через `i64`/`int`-перегрузку), тогда как `ro u = 0xFFFF… as u64; u.to_i128()` — верно. Обход: **биндить приёмник** перед вызовом. Смежное: `int` и `i64` — РАЗНЫЕ Nova-типы с разными C-типами (`nova_int`=`intptr_t` vs `int64_t`, см. `[reference-nova-int-intptr-not-i64]`), поэтому «одинаковое представление» оправданием не является — резолв обязан идти по ОБЪЯВЛЕННОМУ типу выражения-приёмника. Опасность: рекурсия (1) не даёт ни предупреждения, ни ошибки — только краш. | чекер (резолв метода по типу приёмника-выражения с `as`) | **P2 (тихая рекурсия + молчаливо неверная перегрузка)** |
| `[M-named-tuple-field-accessor-on-call-ice]` | **OPEN, ПРИОРИТЕТ ПОДНЯТ 2026-08-01 (блокирует миграцию BigInt на named tuple — решение владельца, см. [M-bigint-family-migrate-named-tuple]); в очередь сразу после окна унификации операторов.** Было: OPEN 2026-07-27 (найдено при реализации `std/math/int128.nv`).** Доступ к полю named-tuple (D215) НА РЕЗУЛЬТАТЕ ВЫЗОВА роняет codegen в ICE: `(-5 as i64).to_i128().hi()` → `nova: internal error at emit_c.rs:58520: [P67-LEGACY] method call `.hi` return type unknown — checker must annotate (compiler-conventions.md §0); obj_ty="NovaTuple_i128" obj=Call`. То же поле через биндинг (`ro x = …; x.hi`) работает. Класс — P67-LEGACY (чекер не аннотировал канал для field-аксессора на Call-выражении named-tuple). Обход: биндить результат вызова. **ICE, а не диагностика** — по §4а compiler-conventions это дефект независимо от редкости формы. | чекер (аннотация канала) + codegen (`emit_c.rs` ~58520 P67-LEGACY) | **P2 (ICE)** |
| `[M-bitwise-operator-method-naming]` | **OPEN 2026-07-27 (вопрос владельца: «@bitand и @and — сравни, возможно назвали неверно»).** Спека (03-syntax.md §Mapping) закрепляет `a & b`→`@and`, `a | b`→`@or`, `a ^ b`→`@xor`, `!a`→`@not`. **Инстинкт владельца обоснован — есть ДВА разных вопроса.** **(A) Именование `@and`/`@or` — кандидат на `@bitand`/`@bitor`/`@bitxor`.** В Nova `&`/`|` — ЧИСТО побитовые (логические `&&`/`||` в таблице отсутствуют, т.е. не перегружаются, как в Rust), но имена `and`/`or` в прозе и в большинстве языков читаются как ЛОГИЧЕСКИЕ. **Rust — наш заявленный эталон архитектуры — намеренно префиксует: `BitAnd::bitand`/`BitOr::bitor`/`BitXor::bitxor`** именно чтобы отделить от логических. Python `__and__` без префикса, но там `and` — не перегружаемое ключевое слово. **Цена переименования СЕЙЧАС ≈ ноль: реализаций `@and`/`@or`/`@xor`/`@not` в std НЕТ ВООБЩЕ** (греп = 0; `std/math/int128.nv` — первый потребитель), позже будет дорого. **(B) `@not` для `!` — отдельный, более острый вопрос (НЕ переименование, а дыра).** Проверено пробами: `!` в Nova — ЛОГИЧЕСКОЕ отрицание (на `bool` работает, на целых нет), **отдельного побитового `~` в языке НЕТ** (греп парсера/спеки = 0). Значит побитовое дополнение НЕ ИМЕЕТ оператора вообще, и привязка его к `@not` семантически неверна (это привязка к логическому `!`). std живёт обходом `x ^ 0xFFFFFFFF` — `checksums/crc32.nv:74,103`, `crypto/md5.nv:196` (там прямо комментарий `// ~B`). **Варианты по (B):** ввести `~` с методом `@bitnot`; ЛИБО оставить обход и зафиксировать в спеке, что побитового НЕ нет как оператора. **Сделано в i128 до решения:** `@bitnot()` — обычный метод, к `!` НЕ привязан (комментарий в коде объясняет). `@and`/`@or`/`@xor` оставлены по действующей спеке. **РЕШЕНО владельцем 2026-07-27: (A) переименовать + (B) ввести `~` перегружаемым.** D46-амендмент ВНЕСЁН в `spec/decisions/03-syntax.md`; реализация — план [234](234-bitwise-operator-family.md) (Ф.0 спека+i128 ✅ сделано; Ф.1 переименование диспетчера, Ф.2 оператор `~`, Ф.3 миграция 9 обход-сайтов). Исполнять ОДНОЙ волной с `[M-named-tuple-operator-overload-incomplete]` — та же таблица диспетчера. | спека (03-syntax.md §Mapping) + std (0 сайтов) + `std/math/int128.nv` | **P3 (именование; окно дешевизны открыто, пока реализаций нет)** |
| `[M-named-tuple-operator-overload-incomplete]` | **OPEN 2026-07-27 (найдено при реализации `std/math/int128.nv`).** Для named-tuple типов (D215, `type X(a T, b U)`) операторная диспетчеризация в `emit_c.rs` **неполна**: ветка `NovaTuple_` (~33656) матчит ТОЛЬКО `Eq|Neq => "equal"`, `Add => "plus"`, `Sub => "minus"`, `Mul => "times"`, `Div => "div"`, а всё остальное падает в `_ => ""` → overload НЕ ищется → эмитится СЫРОЙ C-оператор на структуре → **CC-FAIL** `invalid operands to binary expression ('NovaTuple_X' and 'NovaTuple_X')`. **Выпали (полный список, проверен пробами 2026-07-27):** `%` · `& | ^` · `!` · `< > <= >=` — лоуэрятся ТОЛЬКО `+ - * /` и `== !=`. Подробно: (1) **`%`** (`BinOp::Mod => "rem"`) — при том, что маппинг ЕСТЬ и в спеке (03-syntax.md:2823 «`a % b` → `@rem(b)`»), и в BinOp-таблице компилятора (`BinOp::Mod => "rem"`), и в СОСЕДНЕЙ ветке того же файла (~33741) — то есть это пропуск в одной ветке, не дизайн; (3) **`&` `|` `^` `!`** (`@and`/`@or`/`@xor`/`@not`) — проверено на `std/math/int128.nv`; (2) **`<` `>` `<=` `>=`** (`@compare`-производные) — тип с `@compare` не сравнивается операторами. **Минимальное репро (6 строк, воспроизведено):** `type Pair(a int, b int)` + `@div` + `@rem` + `@equal` → `Pair(10,20) / Pair(3,7)` компилируется, `Pair(10,20) % Pair(3,7)` → CC-FAIL; аналогично `@compare` + `Pair(1,2) < Pair(3,4)` → CC-FAIL. **Записи (`type X value { … }`) не затронуты** — у них своя ветка с полным набором. **Фикс:** дополнить match в `NovaTuple_`-ветке `BinOp::Mod => "rem"` + `Lt/Gt/Le/Ge` через `@compare` (зеркало record-ветки ~33737-33741). Однострочная часть (`Mod`) — тривиальна. **Обход в коде:** звать `.rem(b)` / `.compare(b) < 0` методами (работают). **Ловится ТОЛЬКО на CC** — чекер пропускает, что само по себе вторая половина дефекта (нет `E_*`-диагностики на неперегруженный оператор). | codegen (`emit_c.rs` ~33656 named-tuple binop dispatch) + чекер (нет диагностики) | **P2 (D215-типы теряют часть операторной поверхности)** |
| `[M-missing-effect-handler-segfaults]` | **OPEN 2026-07-27 (вопрос владельца «при отсутствии эффекта падает как — понятно или NPE?» — измерено интегратором пробами).** Вызов эффект-опа без установленного `with` даёт **SEGFAULT (exit 139), БЕЗ единого диагностического сообщения** — не паника с текстом, не «no handler installed», а сырой крах (по форме — NULL-разыменование слота хендлера). **Пробы (бинарь `b24babbc1`):** `type Store effect { save(id str) -> int }` + `export fn inner(id str) Store -> int => Store.save(id)`, вызов из `main` БЕЗ `with` → компилируется чисто, при запуске segfault; stdout пуст (не долетел даже `println` до вызова — буфер потерян на крахе). Под `--strict-effects` — то же (флаг проверяет ОБЪЯВЛЕНИЕ эффекта, не наличие реализации). **Что компилятор ловит (для контраста, работает):** `E_UNDECLARED_TRANSITIVE_EFFECT` — вызов эффектной fn из fn, не объявившей эффект (только под `--strict-effects`); `E_RAW_EFFECT_OP_UNDECLARED` — сырой оп в `export fn` (№131). **Чего НЕ ловит:** «эффект объявлен корректно по всей цепочке, но `with` нигде не поставлен». **Почему важно:** забыть `with` — типовая ошибка (владелец: «мы ловили падения»), а Polaris-паттерн 222.20 A/B/C штатно требует `with SessionStore = …` вокруг accept-loop; ошибка даёт segfault вместо подсказки. Для языка, продающего compile-time-безопасность, сырой крах на частой ошибке — худший из режимов отказа. **Минимальный фикс (дёшево, независим от дизайн-развилки):** NULL-слот хендлера → чистая паника с текстом «no handler installed for effect `Store` — оберни вызов в `with Store = …`» (+ имя опа/спан). **Полный фикс (дизайн, см. 222.20 Q1):** whole-program-проверка достижимости «main → оп без объемлющего `with`» — Nova компилирует одним CU, анализ осуществим; даёт гарантию effect-polymorphic-Handler'а без вирусности эффект-строк в типах. `#default_handler` (D431) закрывает случай эффектов с осмысленным дефолтом (Log/Time), но НЕ ресурсы (SessionStore/Db — там отсутствие дефолта корректно, крах — нет). | рантайм/codegen (эмиссия вызова опа, NULL-слот) + опц. чекер (whole-program reachability) | **P1 (сырой segfault без диагностики на типовой ошибке)** |
| `[M-libuv-unconditional-link-binary-size]` | **OPEN 2026-07-25 (измерено интегратором при size-профиле Hello World).** Каждый Nova-бинарь безусловно линкует libuv (event-loop), даже программа без I/O/concurrency. **Доказательства (`nova build` println-only hello, `hello.exe` 1.00 МБ):** строки `libuv`/`uv_loop` в бинаре; DLL-импорты `WS2_32`(Winsock)/`IPHLPAPI`/`USERENV`/`bcrypt`/`dbghelp` — транзитивные Win-зависимости libuv, не println; секции `.text` 803 КБ + `.rdata` 156 КБ. libuv.lib = 1012 КБ, gc.lib(Boehm) = 2.6 МБ (архивы; линкер тянет использованный срез). Корень: `nova_runtime_init` (`nova_rt/runtime.c`) трогает default event-loop безусловно; `test_runner.rs:1375` линкует libuv «если libuv config present» — а он всегда detected/built в workspace. **Оценка выигрыша:** ленивая линковка libuv/Vela (не тянуть event-loop в программу без spawn/scope/net) убирает ~300-400 КБ → hello ~1.0 МБ → ~0.6 МБ (−40%). **Сложность:** разнести «программа с I/O/concurrency» (нужен loop + libuv) и «чистая» (только GC + std) — `nova_runtime_init` сейчас предполагает наличие loop; нужен анализ «использует ли программа Net/spawn/scope» на этапе линка + условный `-DNOVA_USE_LIBUV` + рантайм-init без loop для чистого случая. **Смежное:** та же линия, что «lazy-link Vela» (size-обсуждение) и план 224 (Vela как подсистема). Boehm GC не трогать (нужен любой аллокации). Сравнение: Rust hello 0.12 МБ (нет GC+нет loop+динамич.CRT), Go 2.35 МБ (весь runtime статикой), Nova 1.0 МБ (GC + безусловный libuv). | codegen/линк (`test_runner.rs` link-step + `nova_rt/runtime.c` init) — условная линковка по анализу Net/concurrency-использования | **P3 (размер бинаря; −40% hello)** |
| `[M-fmt-rich-spec-primitive-fresh-sb-redundant]` | ✅ **ЗАКРЫТО 2026-07-24 (sonnet, worktree `nova-fmtdirect`, ветка `p-fmtdirect`).** Все ПЯТЬ примитивных helper'ов (`emit_int/f64/char/bool/str_display_spec_call`, `emit_c.rs`) теперь пишут резолвленный `*_display_spec`-вызов ПРЯМО в переданный `sb` (никакого fresh `Nova_StringBuilder_static_new(16)` + `consume_into_str` + `append`) — та же девиртуализация, что Plan 208 Ш3 уже применил к ГОЛОМУ примитивному пути, расширенная на RICH-spec ветку. Диспетчер `emit_format_spec_value` сменил возврат `Result<String,_>` → `Result<Option<String>,_>`: `None` для всех пяти примитивных веток (уже записано, append не нужен — caller `emit_interpolated_str` пропускает append по образцу голого `continue`), `Some(expr)` — ТОЛЬКО у композит/юзер-tail (не тронут, V1-упрощение #1 остаётся). **Ф.0 подтверждено чтением `std/src/runtime/string_builder.nv`:** `mark = sb.byte_len()` берётся СВЕЖИМ на каждом вызове независимо от предыдущего содержимого `sb` → корректность на НЕПУСТОМ shared SB тождественна fresh (str/char-Debug quote+escape строится в СВОЙ scratch ДО записи в `sb` при `mark`; f32 rich по-прежнему идёт через widening в `f64_display_spec` — не задето этой волной; zero_pad+sign mark-relative арифметика не зависит от состояния `sb` до `mark`). **Гейт (targeted, без мега-CU):** существующие `d422_f4r_baseline_{int,float,strcharboolu64}.nv`, `d229_debug_format_spec.nv`, `d374_write_sink_decouple.nv`, `d422_generic_container_derive.nv`, `d422_generic_interp_dispatch.nv`, `d422_unified_display_dispatch.nv`, `d186_interp_no_display_pos.nv`, neg `d422_unknown_spec_neg.nv` — ВСЕ PASS байт-в-байт (та же C API семантика, C-текст неизбежно другой). Новая `spec_tests/conformance/fmtdirect_rich_primitive_mixed.nv` — str/char/bool/binary-width оси, которых не было в Ш0-baseline, + MIXED multi-rich-field интерполяция (несколько rich-полей в одной строке подряд — mark-relative стресс-тест) — PASS. **Подсчёт аллокаций:** `_nv_fmt_sb_*`-деклараций в сгенерённом C для этих фикстур — было 43, стало 5 (остаток — исключительно композит/юзер-tail, не задет). `std/src/runtime` targeted-набор (char/string/string_builder/sync _test.nv + fmt_buf/core) — 5/5 PASS, δ0. Спек-амендмент не требуется (поведение побайтово идентично — codegen-оптимизация, не смена языка). | codegen (`emit_c.rs` `emit_int/f64/char/bool/str_display_spec_call` + `emit_format_spec_value`) | ✅ ЗАКРЫТО |
| `[M-manual-collect-lint-missing]` | **OPEN 2026-07-24 (наблюдение владельца по `nova-http/src/url.nv:145`: `mut chars []char = []char.new(); for c in scheme.chars() { chars.push(c) }` → должно быть `mut chars = scheme.chars().collect()`).** Проверено: `scheme.chars().collect()` компилируется БЕЗ аннотации (тип `[]char` выводится) и РАВНО ручному циклу (пробы A/B/C PASS; `.chars()` -> `CharsIter` ленивый, `.collect()` материализует). Линта нет (`W_MANUAL_COLLECT` в lints.rs = 0). **Дрейф:** ручной `mut v T = T.new(); for x in <iter> { v.push(x) }` вместо канона `mut v = <iter>.collect()`. **Инвентарь:** общая форма (push голого loop-var) — **37 сайтов в 23 файлах** (`unicode/collate.nv` x7, `time/cron.nv` x3, `unicode/case.nv` x3, `text/regex.nv`, `nova-http/url.nv`+`server_router.nv`…); из них chars-подформа — 9. **Условие БЕЗ ложных срабатываний (AST):** (1) `v` объявлена пустым конструктором коллекции (`[]T.new()`/`Vec[T].new()`/пустой литерал) НЕПОСРЕДСТВЕННО перед циклом; (2) тело цикла — РОВНО `v.push(<loop_var>)`, один statement, голая loop-переменная (не `f(x)`, не под условием); (3) `v` не используется между объявлением и циклом → `mut v = <iter>.collect()`. **Расширения (позже, как coalesce-семья по форме):** `{ v.push(f(x)) }` → `.map(f).collect()`; `{ if c(x) { v.push(x) } }` → `.filter(c).collect()` — не в первой версии, только identity-collect. Инфраструктура: `ConvRule` в `lints.rs`, семья `W_MANUAL_MIN_MAX`/`W_MANUAL_CLAMP`/`W_MANUAL_COALESCE`/`W_MANUAL_SLICE_TO_END` (тот же класс «ручная форма vs канон», clippy `manual_collect`/`needless_collect`). Id — `W_MANUAL_COLLECT`. **Часть [200 Пункт 22](200-std-improvements.md).** | линты (`lints.rs`, новое ConvRule) + миграция 37 сайтов | **P2** |
| `[M-manual-slice-bounds-lint-missing]` | **OPEN 2026-07-24 (владелец: «`after_scheme[2..]` работает — линт должен ругаться на длинную запись»).** Открытые формы диапазона канон (спека 02-types.md:6761): `arr[a..]` (до конца, end=len), `arr[..b]` (от начала, start=0), `arr[..]` (весь). Проверено: `s[2..] == s[2..s.byte_len()]`, `v[1..] == v[1..v.len()]` (PASS). Линта нет (`W_MANUAL_SLICE_TO_END` в lints.rs = 0). **Три редукции:** (1) `recv[a..recv.len()]` / `recv[a..recv.byte_len()]` -> `recv[a..]` (**37 сайтов** std+nova-http: `data/semver_range.nv` x7, `encoding/url.nv` много, `fs/path.nv`, `math/complex.nv`); (2) `recv[0..b]` -> `recv[..b]` (симметрично, `url.nv:98,152`); (3) `recv[0..recv.len()]` -> `recv[..]`. **Условие БЕЗ ложных срабатываний:** end — ГОЛЫЙ вызов `.len()`/`.byte_len()` на ТОМ ЖЕ receiver-выражении, что и срез (не `x[a..y.len()]` — другой receiver; не `x[a..x.len()-1]` — арифметика); для (2) start — литерал `0`. `.byte_len()` для str, `.len()` для Vec/slice — матчить по факту вызова, тип знать не нужно. Инфраструктура: реестр `ConvRule` в `lints.rs`, семья `W_MANUAL_MIN_MAX`/`W_MANUAL_CLAMP`/`W_MANUAL_COALESCE` (последний влит 2026-07-24 — прямой прецедент, тот же класс «ручная форма vs канон», clippy). Id — `W_MANUAL_SLICE_TO_END`. Fix-it машинный (замена однозначна). **Часть [200 Пункт 22](200-std-improvements.md).** | линты (`lints.rs`, новое ConvRule) + миграция ~40 сайтов | **P2** |
| `[M-statuscode-u16-cast-bounce]` | **OPEN 2026-07-23 (вопрос владельца по `type StatusCode value { priv code u16 }`).** Поле-обёртка хранит `u16`, а `ServerResponse.status` — голый `int`, поэтому статус ходит через каст-мост `.as_u16() as int` — **7+ сайтов** (`server/respond.nv` ×5: text/html/bytes/empty/redirect; `client/wire.nv`, `client/client.nv`) + обратный `as u16` на парсинге. `int` в Nova адресного размера (`nova_int`), u16 экономит 6 байт/значение, но статусы не хранятся массово → выигрыш ноль, цена (касты) реальна: u16 импортирован из Rust (`http::StatusCode`=`NonZeroU16`) без рустовской выгоды. **Разобрано пробами:** newtype `type Status int` инвариант НЕ держит (`Status(999)` и голый `999` втекают D55-коэрсией — sc2/sc3 PASS), поэтому запись с `priv`+валидатором оправдана как форма; но ширина поля должна быть `int`. Enum отвергнут по существу (HTTP-статус — открытое множество). Именованных констант `StatusCode.OK`/… СЕЙЧАС ноль (`export const` в status.nv = 0). **Фикс — в [222.5 §4а](222.5-respond.md):** поле `int` не `u16` + константы + унификация `ServerResponse.status: StatusCode` (убрать тройное представление: StatusCode / ServerResponse.status int / client Response.status int). Behavior-adjacent → миграция сайтов, int-формы конструкторов депрекейтятся до удаления. | nova-http (`status.nv`, `respond.nv`, `server.nv`) — план 222.5 | **P2** |
| `[M-ro-launder-via-mut-binding]` | **OPEN 2026-07-23 (поднято владельцем: «параметры по умолчанию ПОЛНОСТЬЮ ro, это не про биндинг; параметр не может быть присвоен в mut-переменную»).** L1-ro (параметр по D176-дефолту ЛИБО явный `ro`-локал) **полностью невидим для coercion**: ro-связанное кучевое значение свободно затекает в любую mut-позицию, и запись видна оригиналу/вызывающему. **ВАЖНО — это НЕ баг против действующей спеки** (первичный вывод интегратора был неверен, исправлено после дочитывания P8): **P8 сформулирован дословно как «coercion по оси content (L2), НЕЗАВИСИМО от L1»** — то есть текущее поведение компилятора P8 буквально СОБЛЮДАЕТ; а P10 («`ro` = per-path write-ban, не object-freeze») его отчасти санкционирует. Дыра — в САМОЙ СПЕКЕ: P7 морозит owned-граф ЧЕРЕЗ биндинг, P8 при переносе в другой биндинг L1 игнорирует → заморозка не переживает ре-биндинг. **Следствие: закрытие = ЯЗЫК-ИЗМЕНЕНИЕ, обязателен D246-амендмент в том же слиянии** (не «просто фикс чекера»). **Чего в спеке НЕТ (указано владельцем):** таблицы допустимых КОНВЕРСИЙ по парам (L1,L2)источник × (L1,L2)цель. Существующие таблицы D246 (binding×content, параметры, указатели L1×L3) описывают права доступа ВНУТРИ одного биндинга, а не перенос между биндингами; весь перенос свёрнут в одну прозаическую строку P8, покрывающую только L2 и L3. **Матрица проб (бинарь `6221c669b`)** — что ловится и что нет: (B) прямая запись `v[0]=99` в параметр → ✅ `E_READONLY_CONTENT` (ORACLE F3 жив); (J) L2-типовой `-> ro []int` → `mut b []int` → ✅ `E_READONLY_COERCE` (L2-ось работает); (A) `fn f(v []int) { mut w = v; w[0]=99 }` → ❌ компилируется, у вызывающего `arr[0]`=99; (D) `ro a = [1,2,3]; mut b = a; b[0]=99` → ❌ ORACLE F1 обойдён — дыра НЕ параметро-специфична; (F) `Vec[int]`-параметр → `mut w = v; w.push(..)` → ❌ Vec вызывающего вырос; (G) value-запись с кучевым полем (**точная форма `ServerResponse { headers HeaderMap, body []u8 }`**) → `mut resp = r; resp.header(..)` пишет в HeaderMap ВЫЗЫВАЮЩЕГО; (H) **`fn outer(v []int) { fill(v) }` при `fn fill(mut v []int)` → ❌ ПРОХОДИТ — отмывание работает и в позиции АРГУМЕНТА, промежуточный `mut w = v` даже не нужен**; (I) `ro a = [..]; fill(a)` → ❌ то же на ro-локале; (E) чистый value-record без кучевых полей → ✅ копия независима, вреда нет — **единственная ветка, которую закрывать НЕ надо**. **НОРМА (решение владельца 2026-07-23, СТРОГАЯ, послаблений нет):** L1-ro источник (`ro a T` ЛИБО параметр `a T` по D176-дефолту) в mut-цель — **`E_READONLY_COERCE` во ВСЕХ позициях** (инициализация биндинга · аргумент вызова · возврат) и для **ВСЕХ типов**. Владелец явно отверг позиционное послабление («разрешить L1-ro→mut для локальной копии — ни в коем случае»); классовое послабление (разрешить чисто-значимым типам, проба E) интегратор предлагал в черновике — **снято**: (1) противоречит категоричной формулировке владельца «параметр не может быть присвоен в mut-переменную»; (2) проба G доказала, что «value-тип» вообще НЕ является признаком безопасности — value-запись с кучевым полем течёт, значит предикат пришлось бы делать рекурсивным по полям и он остался бы хрупким на generic/`Option[Vec[T]]`; строгая норма снимает весь этот класс краевых случаев и не требует предиката класса хранения вовсе. **Санкционированная дверь для «нужна изменяемая копия»: явный `.clone()`** (протокол `Clone`, D230 — `Vec`/`HashMap`/`Set`/`StringBuilder`/`str` уже несут `@clone()`); стоимость копии становится видимой в коде. Побочный эффект нормы: типы БЕЗ `Clone`-реализации потребуют её добавления либо реструктуризации сайта — инвентарь таких типов снимается в Ф.0. Источник L2-ro (`ro T`) в mut-цель — запрещён уже сейчас (проба J), без изменений; источник L1-mut — разрешён. **Проба E перестаёт быть pos-фикстурой и становится neg.** **Объём миграции — БОЛЬШЕ, чем казалось:** не только 246 сайтов `mut X = <идентификатор>` в `std/src`+`examples`+`nova-http/src`, но и КАЖДЫЙ вызов, передающий ro-связанное кучевое значение в `mut`-параметр — инвентарь не снят. **Следствие для кода:** паттерн `mut X = X0` (`nova-http/src/middleware/cors.nv` `decorate_simple`, `nova-http/src/client/client.nv:332-335`) держится на этой дыре — там аргумент временный, ущерба нет, но форма течёт. **Порядок работ:** Ф.0 D246-амендмент (таблица конверсий (L1,L2)×(L1,L2)) → Ф.1 чекер → Ф.2 миграция. Behavior-changing → мега-CU conformance полным фильтром + флагман-examples. **ПРОГРЕСС 2026-07-23 (worktree `nova-rolaunder`, ветка `p-ro-launder`, [Plan 224](226-ro-launder-l1-coercion.md)):** D246-амендмент СДЕЛАН (таблица конверсий + ORACLE G/H + P8-ретракт); чекер (Ф.1) СДЕЛАН для 2 из 3 позиций (`let`-init аннотированный/неаннотированный + call-argument free-fn/метод) — позиция ВОЗВРАТА НЕ реализована (новый маркер `[M-ro-launder-return-position-unimplemented]`). Побочная находка/фикс: `fn_mut_params`/`method_mut_params` конфлируют overload'ы ПО ИМЕНИ — без guard'а ломали зелёный D326 mode-overload conformance-тест (`d326_mode_overload_axis.nv`) false-positive'ом; исправлено (`fn_overload_names`/`method_overload_names`, новый маркер `[M-mut-params-registry-overload-conflation]` — тот же корень может задевать `check_unsafe_coerce_args`, не проверено). **HARD-STOP сработал ПОСЛЕ Ф.1 (не было надёжного способа снять точный инвентарь ДО чекера regex'ом):** реальный объём — `std/src` 721 хитов/165 файлов (baseline 27), `examples` 197/47, `nova-http/src` 345/41 (соседняя репа, не мигрировалась) — суммарно **1263**, на порядок больше предполагавшихся 246. Абсолютное большинство в `std/src` — НЕ кучевые launder'ы, а копии СКАЛЯРНЫХ примитивов (`mut end = n` для `int`), для которых `.clone()`-дверь бессмысленна (копия int уже независима). Провизорная scalar-primitive exemption измерена (721→13 в `std/src`) и явно ОТКЛОНЕНА координатором в реальном времени («всё ❌ кроме `.clone()`») — удалена из кода. Новый маркер `[M-ro-launder-scalar-exemption-question]` — открытый вопрос владельцу: вернуть scalar-exemption или организовать отдельную волну на ~1000+ механических (но требующих ручного решения per-site) правок. Ф.2 массовая миграция НЕ начата (кроме 7 spec_tests-фикстур, найденных как false-positive-verification побочный продукт — `blanket_fold.nv`, `d228_value_record_copy_contract.nv`, `m_embvt_protocolbox_embed_callarg_ok.nv`, `value_semantics.nv`, `neg/mut_param.nv`, `standalone/f2_protocol_dispatch_method_survives.nv`; `d246_param_ro_mut_view.nv` — тот же класс, НЕ мигрирован). Детали — Plan 224 §5. **ЗАКРЫТИЕ 2026-07-23 (тот же сеанс, координатор подтвердил финальную норму дважды):** scalar-primitive exemption ВОЗВРАЩЕНА как ЧАСТЬ финальной нормы (не уступка — явное решение владельца: скаляр-примитив ≠ value-запись, проба E остаётся neg); диагностика получила канон-порядок подсказок (mut-параметр первым — D326-ревизия §Р3, `.clone()` вторым). Возврат-позиция (Ф.1б) дореализована (`check_ro_launder_return*`, зеркалит `check_closure_scalar_return*`); попутно найден и пофикшен variadic-param false-positive (`Vec[T].of(...args) => args` — vararg-массив всегда свежий, exemption добавлен). Ф.2 миграция ЗАВЕРШЕНА для `std/src` (13 сайтов, 721→0), `examples` (собственный код, 0 хитов), `spec_tests/conformance` (20 файлов — 6 исходных + 14 найденных return-позицией, преимущественно bare generic-identity `fn[T](x T)->T=>x` паттерн с mut-параметр фиксом). Гейты зелёные: `nova check std/src` `PASS:142 FAIL:27` byte-identical baseline; `nova check examples` `PASS:46 FAIL:1` (1 — network git-fetch flake, не мой код); `nova test` батчами по всем touched-директориям — все PASS (несколько директорий, `net`/`identifiers`/часть `crypto`+`testing`, блокированы **pre-existing** ICE `[P67-LEGACY] method=now`, воспроизведено на baseline — см. `[M-fn-newtype-return-position-broken]`, №53 в 221.1). nova-http (345 хитов) — вне мандата, инвентарь передан, новый маркер `[M-ro-launder-nova-http-migration]`. Полные детали, дословные вердикты, список всех правленых файлов — Plan 224 (переписан целиком под финальный статус). Маркер этой строки — CLOSED в части std/examples/spec_tests; nova-http и bound-aware-анализ — отдельные follow-up маркеры. **§72-followup ЗАКРЫТ 2026-07-24** (компиляторное окно №72, worktree `nova-fsval`, ветка `p-fullstack-value`, sonnet, [Plan 226](226-ro-launder-l1-coercion.md) §9): scalar-primitive exemption РАСШИРЕНА до «полностью-стековый value-тип» — рекурсивный `is_fully_stack_value`/`_name` заменил `is_bare_scalar_primitive`/`_name` на всех трёх позициях; `str` — отдельное immutability-исключение (D26); проба G подтверждена neg (граница не сдвинута); проба E RECLASSIFIED neg→pos (fullstack-value квалифицируется); D246-амендмент §72 в spec/decisions/02-types.md; `nova check std/src` байт-идентично baseline (142/27/1040). | спека D246 (P7/P8/P10, отсутствует таблица конверсий) + чекер (`assignable`, `types/mod.rs`) | **P1 (звучность модели мутабельности) — ЗАКРЫТО (std/examples/spec_tests + §72 fullstack-value-exemption); nova-http/bound-aware — follow-up** |
| `[M-fn-newtype-return-position-broken]` | ✅ **ЗАКРЫТО 2026-07-23 (sonnet, worktree `nova-n53`, ветка `p-fix-n53-fnret`, срочное мини-окно №53).** Корень всех трёх форм (а)/(б)/(в) — ОДИН: `fn_returns_fn_sig` (codegen, `emit_c.rs`) регистрировался только для ЛИТЕРАЛЬНОГО `-> fn(...) -> ...` возврата (`if let Some(TypeRef::Func{..}) = &f.return_type`), а `fn make() -> Handler` (newtype-возврат) не матчился вовсе — тот же `resolve_fn_typeref`, что 4c26fe2a0 уже применял к параметрам, здесь применён к f.return_type ВПЕРВЫЕ. Форма (г) — отдельный, но родственный пробел: голый `@` лежит в AST как `ExprKind::SelfAccess` (не `Ident("self")` — та явная форма уже работала), и ни `infer_call_ret_c`, ни `emit_call`'s dispatch не имели ветки для звонка ПО этому узлу. **Фикс (4 точки, `emit_c.rs`):** (1) `emit_fn_forward_decl` — `f.return_type` резолвится через `resolve_fn_typeref` ПЕРЕД проверкой на `Func`, закрывает fn_returns_fn_sig для newtype-возврата (корень формы б); (2) `infer_call_ret_c` — две новые ранние ветки: func=вложенный `Call` к именованной fn с записью в `fn_returns_fn_sig` (формы а/в) И func=`SelfAccess` с receiver-типом в `fn_newtype_sigs` (форма г) — обе ДО существующих Member/Path веток, не маскируют ничего (структурно непересекающиеся AST-формы); (3) `emit_call` — зеркальные ветки на эмиссии, через новый общий хелпер `emit_clos_call_dispatch` (те же `NOVA_CLOS_CALL_*`/`NovaClosBase`-каст, что уже был инлайнен для `Ident`+`fn_param_sigs`); чейн-вызов (а/в) хоистится во временную `void*` перед диспетчем (ИНАЧЕ внутренний call эмитился бы ДВАЖДЫ — экономия для `->fn`/`->env`, безвредная для простого `Ident`, но реальный дубль-вызов для вложенного `Call`); self-call (г) использует `(*nova_self)`, НЕ `nova_self` — `receiver_c_type`'s generic-fallback оборачивает fn-newtype receiver в ЛИШНИЙ указатель (`Nova_X*` = `void**`, тогда как значение — голый `void*`), нужен один deref. **Побочный фикс той же природы:** `emit_call`'s "4b is_self_ref" ветка (существующий self-referential void*-диспетч, e.g. `t.length()` внутри `LinkedList.length`) делала тот же ошибочный cast `(Nova_X*)(obj_c)` БЕЗ адаптации под fn-newtype receiver ABI — исправлено (хоист во временную + `&tmp`), гейт `self.fn_newtype_sigs.contains_key(recv_ty)` — байт-идентично для всех НЕ-fn-newtype типов (heap record/sum — старая ветка не тронута). **Фикстуры (RED→GREEN, 4 файла + δ0):** `spec_tests/conformance/d52_fnret_a_call_chain.nv` (форма а), `d52_fnret_b_let_bind.nv` (форма б, оба варианта — неаннотированный И аннотированный биндинг), `d52_fnret_c_mw_chain.nv` (форма в, middleware-форма с closure-декорированием — identity-return `h` намеренно избегался, отдельно наступает на [M-ro-launder-via-mut-binding], несвязанный класс), `d52_fnret_d_self_call.nv` (форма г — caller-side намеренно через STATIC-метод на том же типе, self-ref диспетч; ВНЕШНИЙ dot-call именованного метода на произвольном void*-erased fn-newtype-значении — см. новый `[M-fn-newtype-method-dispatch-void-star-external-gap]` ниже, НЕ входил в объём формы (г) — та строго про `@(v)` ВНУТРИ тела метода). **Регресс:** targeted-CU (5 новых assert-тестов + существующая `d52_newtype_fn_type.nv` δ0, 9/9 PASS вместе, byte-identical для δ0); `std/src/collections` (self-referential void*-диспетч на heap-типах — LinkedList и родня — не задет) `nova test` PASS 13/0 SKIP 7; `cargo test --lib codegen::emit_c` — 76 passed/2 failed, **оба сбоя ПОДТВЕРЖДЕНЫ pre-existing на немодифицированном main** (не регрессия — `array_lit_named_tuple_box_tests`, prelude/int-collapse, никак не связаны с fn-типами); `examples/flagship/aggregator --strict-effects` — 1 pre-existing FAIL в nova-http (`[M-ro-launder-via-mut-binding]` nova-http-follow-up, не мой код, не тронут). Полный `spec_tests/conformance` мега-CU НЕ гонялся (CPU-дисциплина срочного окна — targeted only); авторитетный гейт — интегратор. | codegen (`emit_c.rs`: `resolve_fn_typeref` на f.return_type + chain-call/self-call dispatch + is_self_ref receiver-ABI) | ✅ ЗАКРЫТО |
| `[M-fn-newtype-method-dispatch-void-star-external-gap]` | **OPEN 2026-07-23 (найдено ПОПУТНО при верификации №53 формы г, вне мандата этого окна).** Именованный метод (не call-through, не `@(v)`) вызванный через dot-syntax `h.method(args)` на значении, чей C-тип — голый `void*` (ЛЮБОЙ fn-newtype параметр/локал/call-результат — D52), **всегда** падает в `emit_call`'s "4b" ветку (`obj_ty == "void*"` → трактуется как erased-generic-стаб) и эмитит **`NULL` без единой попытки резолва** — если только не найден `is_self_ref` (текущий `current_receiver_type` совпадает с типом метода, независимо от того, ЯВЛЯЕТСЯ ли obj реальным self). Репро: `fn use(h Handler) -> str => h.some_method(3)` из СВОБОДНОЙ функции (не метода на Handler) → `nova_str r = NULL;` без диагностики, БЕЗ ошибки компиляции — silent miscompile (не CC-FAIL/ICE, хуже). Корень: `void*` перегружен ДВУМЯ несовместимыми значениями («erased generic T», для которого NULL-фоллбек корректен, И «конкретный fn-newtype», для которого метод РЕАЛЬНО существует в `method_overloads`/`fn_newtype_sigs`, просто дispatch не пытается его найти) — кода, различающего эти два случая, нет вовсе. Фикс — за пределами узкого №53: нужен дополнительный lookup (по объявленному Nova-типу параметра/биндинга, не по эрозированному C-типу) ПЕРЕД NULL-фоллбеком, когда `obj_ty=="void*"` — т.е. таблица «параметр/локал → исходный `TypeRef`» (частично уже есть для НЕКОТОРЫХ каналов, не унифицирована). Обнаружено НЕ через ICE/CC-FAIL, а через silent-NULL — потенциально ЗАДЕВАЕТ существующий необнаруженный код (аудит не проводился, вне мандата). | codegen (`emit_call` "4b" void*-erasure fallback vs fn-newtype concrete-type) | **P2 (найдено, silent-miscompile класс — не «просто fail», приоритизировать)** |
| `[M-fn-newtype-overload-ambiguity-not-checker-caught]` | **OPEN 2026-07-23 (найдено ОКНОМ-5, тот же класс что `[M-172.1-free-fn-multi-overload-ambiguous]` / ЛОВУШКА `[M-concrete-instance-arity-overload-mangle]` из 222.3 §5).** Два newtype над ОДНИМ fn-типом (`Handler`/`Middleware`, оба `fn(Req) -> Resp`) + два overload'а `process(h Handler)`/`process(m Middleware)` + вызов голой fn-функцией — checker принимает БЕЗ ошибки (не детектирует двусмысленность выбора overload'а), компиляция падает ПОЗЖЕ на C-уровне (`redefinition of 'nova_fn_process'` — оба erased-параметра `void*`, мангл коллидирует). Честно (не silent miscompile), но диагностика хуже некуда (сырой clang-error, не Nova `[E_...]`). Стоп-протокол этого класса уже задокументирован в 222.3 §5 («если споткнётся — чекпоинт + отчёт интегратору») — НЕ трогалось этим окном по этому протоколу. Регресс зафиксирован как `EXPECT_CC_ERROR` (НЕ `EXPECT_COMPILE_ERROR` — тот проверяет ТОЛЬКО стадию nova-codegen .nv→.c, не clang; `spec_tests/conformance/neg/d55_fn_newtype_ambiguous_lift_neg.nv`). | D84 overload resolution / чекер | **P3** |
| `[M-d35-method-value-receiver-mode-unspecified]` | **OPEN 2026-07-23 (вопрос владельца: «@ передаётся по ссылке — как это влияет на тип первого параметра?»).** D35 §Method values (Plan 11 Ф.4) специфицирует unbound-форму как `fn(Receiver, params) -> R`, но ВСЕ примеры — только с `ro`-получателем (`int.@neg`, `Account.@add`). **Не специфицировано:** `mut`- и `consume`-получатели. Должно быть (терминология D326-ревизии Plan 184, уточнение владельца 2026-07-23): тип несёт **РЕЖИМ** параметра `{ro, mut, consume}` — по Р13/Р14 это **ось перегрузки**, единая с receiver-mut; а **`ref`** (ограниченный тип `TypeRef::Ref`, Р1-ревизия — формы `mut ref`/`ro ref` в сигнатуре УДАЛЕНЫ) — это **размещение, которое решает КОМПИЛЯТОР** (авто-`ro ref` + heap↔stack, Plan 172.4 / Q-value-abi-auto-placement; D315 «ABI выводится») → в тип значения `ref` НЕ протекает. **ТРЕБОВАНИЕ ABI-КАНОНИЧНОСТИ fn-типа (уточнение владельца 2026-07-23 «про ref не забыл?» — пропущенная половина):** раз размещение авто-решается, два значения ОДНОГО поверхностного fn-типа могут иметь РАЗНЫЙ C-ABI (`ro f fn(BigRecord)->int = BigRecord.@method` — получатель авто-`ro ref`, указатель; `ro g fn(BigRecord)->int = some_free_fn` — параметр по значению; `if c { f } else { g }` — один тип, два ABI = miscompile, который тип-чекер не ловит). **Нормативно:** fn-тип имеет ЕДИНЫЙ канонический ABI; сгенерированный wrapper (`NovaClosBase{fn, env}` — уже описан в D35 §C-runtime) ОБЯЗАН быть адаптером, приводящим любое размещение к канону. Это должно быть ЗАПИСАНО требованием в D35-амендменте, а не оставаться следствием текущей реализации; + neg-фикстура на смешение двух источников одного fn-типа (метод с авто-ref-получателем ∪ свободная fn) в одной переменной — `fn Buffer mut @write(s str)` → `Buffer.@write : fn(mut Buffer, str) -> ()` (in-out первый параметр, D326); `fn Body consume @close()` → `Body.@close : fn(consume Body) -> ()` (линейность едет в тип); `-> @` (D409 auto-self-return) → возврат = тип получателя. **Почему важно:** без режима в типе значение `mut`-метода звалось бы на `ro`-значении (дыра мутабельности), а значение `consume`-метода потеряло бы линейность (функция, молча съедающая аргумент). **СЛЕДСТВИЕ, найденное при уточнении (живой случай в std!):** раз режим — ОСЬ ПЕРЕГРУЗКИ, то method-value на методе, перегруженном ТОЛЬКО по режиму получателя, **неоднозначен**. Реальная пара: `fn Vec[T] @ptr() -> *T` (access.nv:262) и `fn Vec[T] mut @ptr() -> *mut T` (:270) → `Vec.@ptr` — какой из двух? Спека §Disambiguation покрывает перегрузки по типу АРГУМЕНТА (лямбда с явными типами), но **режим-перегрузки не покрывает вовсе**. **ФОРМА РЕШЕНА владельцем 2026-07-23 — симметрия с объявлением, полный набор из 4 форм:** `Type.name` = статик; `Type.@name` = unbound-инстанс с **ro**-получателем (голое `@` = ro, как в объявлении `fn Vec[T] @ptr()`); `Type.mut@name` = mut-получатель; `Type.consume@name` = consume-получатель. Типы: `Vec.@ptr : fn(Vec[T]) -> *T`, `Vec.mut@ptr : fn(mut Vec[T]) -> *mut T`, `Body.consume@close : fn(consume Body) -> ()`. Никакого вывода режима по контексту → неоднозначность невозможна ПО ПОСТРОЕНИЮ. **Принятое следствие:** метод, объявленный ТОЛЬКО как `mut`, недоступен через `Type.@name` — это честная ошибка «нет ro-метода», а не молчаливый подбор; **требование к диагностике:** текст ошибки ОБЯЗАН подсказывать правильную форму (`Buffer.mut@write`), иначе форма необнаружима для новичка.
**ТРЕТЬЯ ОСЬ — перегрузка по ПАРАМЕТРАМ (вопрос владельца 2026-07-23): выбор по ОЖИДАЕМОМУ ТИПУ, нового синтаксиса НЕ вводим.** У method-value нет аргументов → единственный источник информации это ожидаемый тип: аннотация биндинга (`ro f fn(mut Buffer, str) -> () = Buffer.mut@write`), тип параметра принимающей функции (`apply(Buffer.mut@write, buf, "x")`), объявленный тип возврата. Нет ожидаемого типа → ошибка неоднозначности с перечислением кандидатов. Обоснование: (1) это УЖЕ механизм языка (D84 «самый специфичный матч»; для значения аргументов нет — остаётся ожидаемый тип), новой сущности не нужно; (2) универсальный консенсус — Rust `let f: fn(&mut Buffer,&str) = Buffer::write`, C# method-group conversion, Swift — все выбирают по target-типу; (3) **две оси ортогональны**: режим задаётся ФОРМОЙ (`Vec.mut@ptr`), параметры — ОЖИДАЕМЫМ ТИПОМ, комбинируются без взрыва синтаксиса (`ro f fn(mut Buffer, []u8) -> () = Buffer.mut@write`). Диагностика при отсутствии ожидаемого типа обязана перечислить кандидатов и показать форму с аннотацией. Всё три оси решить В ТОМ ЖЕ D35-амендменте. Нужен D35-амендмент + проба текущего поведения (возможно уже отвергается, но молча/неверной диагностикой). | D35 Method values / спека + чекер | **P2** |
| `[M-method-value-static-ret-type-ice]` | ✅ GUARD ЗАКРЫТ 2026-07-23 (ОКНО-4, sonnet, ветка `p-okno4`): не фича — честная диагностика вместо тихой деградации. Корень уточнён эмпирически: `ro mk = Account.new` парсится как `ExprKind::Path(["Account","new"])` (НЕ `Member` — как дотнутый qualified-путь), НЕ как заявленный в этой записи `[P67-LEGACY]`-ICE — реальный симптом на текущем компиляторе: `Stmt::Let`-эмиссия (`emit_c.rs`) тихо деградирует до ПУСТОГО C-типа (`infer_expr_c_type` не имеет ветки ни для `Member`, ни для `Path`-формы static-method-значения) → `mk = Account_new;` без типа → CC-FAIL «undeclared identifier» на использовании, на шаг дальше от истинной причины. Фикс: guard в `emit_stmt`'s `Stmt::Let` (emit_c.rs, ПЕРЕД инференсом типа) — детект обеих AST-форм (`Member`/`Path` len=2), сверка с `self.method_overloads` (`MethodSig::is_instance==false` = static) → `return Err("[E_METHOD_VALUE_STATIC_UNSUPPORTED] ... use a closure || Account.new(...) ...")`. НЕ трогает: реальный вызов `Account.new(5)` (не бьётся — is_call_func-эквивалент через AST-форму, не Path/Member-в-значении), unbound-instance `Account.@add` (отдельная, уже рабочая ветка, `name.starts_with('@')` явно исключён). Фикстуры: `spec_tests/conformance/method_value_static_form_guard.nv` (pos — static-call + closure-workaround не регрессят), `spec_tests/conformance/neg/method_value_static_form_neg.nv` (neg — E_METHOD_VALUE_STATIC_UNSUPPORTED). **Побочная находка (НЕ фикшена, вне объёма гарда):** closure, оборачивающий static-конструктор `value`-типа (`|| Account.new(9)` где `Account` — `value`-record), падает В РАНТАЙМЕ (RUN-FAIL, зависание/креш, не просто assert-fail) — тот же workaround на HEAP-record (без `value`) работает штатно; отдельный, узкий, доп. класс (значение+closure-boxing), маркер не заведён отдельно (эта запись — про static-form-value guard конкретно, не про closure-crash). | D35 Method values / codegen-guard | ✅ ЗАКРЫТ (guard) |
| `[M-method-value-blanket-not-found]` | **OPEN 2026-07-23 (там же).** Method value НЕ находит БЛАНКЕТ-методы: `int.@to_str` → честная чекерная ошибка `method value: no method 'to_str' on type 'int'` (диагностика КОРРЕКТНАЯ, не ICE), хотя `5.to_str()` работает через бланкет `fn[T Display] T @to_str()`. **ДИАГНОЗ (уточнение владельца 2026-07-23): это НЕ «lookup не видит бланкеты», а отсутствие ИНСТАНЦИРОВАНИЯ.** `int.@to_str` — не «взять адрес готовой функции», а **инстанцировать бланкет `fn[T] T @to_str()` при T=int** и взять адрес мономорфизации. Для прямого метода (`int.@max`) мономорфизация уже существует — потому и работает. Правильное место фикса — **заявка мономорфизации из method-value-позиции в mono-коллектор** (mono = ФАЗА, не побочный эффект кодогена — rustc-эталон), а НЕ заплатка в резолвере имён. Ограничивает `a.map(int.@to_str)`-идиому именно там, где она наиболее полезна (Display/Ints-бланкеты). | D35 Method values / резолв бланкетов | **P3** |
| `[M-serde-field-attrs-unimplemented]` | **OPEN 2026-07-22 (аудит владельца «serde до оригинала?»).** Plan 180 СПРОЕКТИРОВАЛ полный `#serde`-attr-набор (180-план line 90: «rename/rename_all/skip/default/flatten — эталон»), но `SerdeArg` (ast/mod.rs:910) довёз ТОЛЬКО `Tag`/`Content`/`Untagged` (sum-tagging). Field-атрибуты `rename`/`rename_all`/`skip`/`skip_serializing_if`/`default`/`alias`/`flatten` — НЕ реализованы; `#serde(rename=…)`-обещания 180 §68 аспирационны. Блокирует реальную web-пользу extractors (без `rename_all` camelCase↔snake_case любой фронт-JSON мимо). Довезти в auto_derive.rs synth + attr-парсер. Nova-лучше-Rust: rename_all типизированным enum (compile-checked), default через языковые field-defaults, compile-time валидация всех attr. Home = 222.2 (продолжение 180). | Plan 180/222.2 / auto_derive.rs | **P2** |
| `[M-servemux-routing-placeholder]` | **OPEN 2026-07-22 (аудит).** `ServeMux` (nova-http server.nv) — linear-scan `for r in routes` O(n)/запрос; `{param}` ТОЛЬКО 1 сегмент; НЕТ `{*wildcard}`; **first-match по ПОРЯДКУ РЕГИСТРАЦИИ** (док утверждает «Go-1.22-style» но precedence НЕ реализован — конкретный сегмент не побеждает param); нет nesting/групп/middleware. Не «Go-калька», а огрызок. Router с нуля по Axum (segment-trie + wildcards + precedence + nest + MethodRouter) — Plan 222.1; ServeMux ретайр/фасад. | Plan 222.1 / nova-http | **P2** |
| `[M-concrete-instance-arity-overload-mangle]` | **OPEN 2026-07-21 (П21-исполнение: ловушка №2 брифа стрельнула).** Инстанс-перегрузки по арности на КОНКРЕТНОМ типе дают ОДИН C-символ: `Date @at(TimeOfDay)` + `Date @at(h int, m int = 0, ...)` → оба `Nova_Date_method_at` → `too many arguments to function call, expected 2, have 3` (civil_arith_test.c). Generic-типы манглятся полной сигнатурой (П5/138.4 `Vec @index`-прецедент: «Full-signature overload mangling for generic-type methods»); конкретные — имя-only. Фикс: тот же full-signature-мангл для конкретных инстанс-перегрузок (одно окно с generic-путём). БЛОКИРУЕТ П21-форму `Date @at(h,...)` (отложена, коммент у декла-места в datetime.nv). | codegen mangle | **P2** |
| `[M-static-multi-overload-chain-call-type]` | **OPEN 2026-07-21 (П21-исполнение, вторая находка).** Chained метод-вызов НА РЕЗУЛЬТАТЕ мульти-перегрузочного статика берёт тип НЕ ТОЙ перегрузки: `DateTime.new(2026, Jun, 8, 25, 0).is_ok()` (5-арг → композит -> Result) чекер вывел как `DateTime` (тип 2-арг формы) → `E7320 no method is_ok on DateTime`. При ЕДИНСТВЕННОЙ форме статика chain работает (`Date.new(...).is_ok()` — зелёный сосед). Обход в тестах: биндинг перед вызовом. Семья закрытого `[M-vec-ext-method-untyped-let-breaks-chain-dispatch]` — рецидив для static-multi-overload chain-канала. | checker/callnorm chain-канал | **P2** |
| `[M-d424-rawptr-extern-infer-gap-intra-module]` | **OPEN 2026-07-20 (эмпирический опыт интегратора по вопросу владельца «f64_fmt_into точно не unsafe?»).** D424: extern fn с raw-ptr параметром ⇒ unsafe-to-call ПО ИНФЕРЕНСУ (keyword не нужен — вариант A). Enforcement-волна 174.6 M4 закрыта, НО покрытие неполное: снятие `unsafe{}` вокруг вызова ПРИВАТНОГО `extern "C" fn f64_fmt_into` из соседней fn ТОГО ЖЕ модуля (fmt_buf.nv::fmt_f64) компилируется и проходит (checksums PASS 3/0, проба 2026-07-20, откачена) — E_UNSAFE_CALL_REQUIRES_WRAP не эмитится. Сверить охват закрытой волны: вероятно, инференс-класс собран для export/cross-module форм, а intra-module/private-extern вызов мимо. Фикс: инференс-классификация в ту же A11-карту, где обычные unsafe fn (одно окно). | D424 / checker A11 | **P2** |
| `[M-imports-entry-folder-module-self-cycle-empty-exports]` | ✅ **CLOSED 2026-07-20 (sonnet, worktree `nova-entryfix`, ветка `p-fix-entry-exports`, НЕ влито в main — ждёт интегратора).** Root-cause подтверждён: `resolve_imports_inline_ex` держит `entry_key` в `in_progress` до самого конца функции (imports.rs:723/1133-старой нумерации, включая drain `pending_peer_preludes`), а `resolve_one`'s cycle-guard проверял `in_progress` РАНЬШЕ `visited` — файл, транзитивно затянутый в CU auto-injection'ом (`.ptr()` → `needs_vec_injection` → `std.collections.vec` → peer-прелюдии → `std/prelude/collections.nv` → `string_builder.nv`), который сам импортирует entry-модуль обычным `import` (`string_builder.nv` → `fmt_buf`), получал ПУСТОЙ `visible_acc` для экспортов entry → `undefined identifier` в чужом файле. Фикс (вариант **б** — cycle-guard отдаёт уже известные exports entry, а не вариант **а** снятия entry_key из in_progress перед drain'ом — (а) чинит только ОДНУ фазу-манифестации, не общий класс): (1) экспорты entry вычисляются СРАЗУ после сбора siblings — `module.items` уже полностью распарсен, рекурсия не нужна (новая `exported_names_from_items`, зеркалит `resolve_one`'s per-file `module_has_exports`/`is_export` правило) — и сеются в `visited[entry_key]` ДО import_work-цикла; (2) в `resolve_one` порядок guard'ов переставлен: `visited`-check ПЕРЕД `in_progress`-check — для любого НЕ-entry модуля no-op (инвариант: `in_progress`/`visited` взаимно исключающие, pop+insert атомарны в конце `resolve_one`), для entry — даёт корректный export-набор; (3) финальный `visited.insert(entry_key, vec![])` (затирал кэш пустым вектором) — удалён вместе с ложным инвариантным комментарием. Верификация: `nova test std/src/runtime/fmt_buf.nv` ДО→undefined-каскад (по диагнозу), ПОСЛЕ→PASS 1/0 (8/8 внутренних тестов); `nova check std/src/runtime` PASS 18/0 WARN 121 (0 undefined int_fmt_into); настоящий двусторонний A↔B цикл (НИ один участник не entry) по-прежнему ловится — новая neg-фикстура `spec_tests/conformance/entry_self_cycle/{cyc_a,cyc_b,cycle_test}.nv` (третий файл — entry, `cyc_a`↔`cyc_b` — генуинный цикл) даёт честный `undefined identifier` на незамкнутой ветке цикла, `nova test --compile-error entry_self_cycle` → PASS (negative); `std/src/checksums` PASS 3/0, `std/src/collections` PASS 13/0; флагман `examples/flagship/aggregator/src/main.nv --strict-effects` built чисто; folder-CU `nova test spec_tests/conformance --jobs 4` PASS 126/0 SKIP 16, `--compile-error` лейн PASS 385/0 (включая новую фикстуру) — оба FAIL:0, known-red не встретилось. Коммиты (ветка `p-fix-entry-exports`): `160789715` (фикс `imports.rs`) + `b8385f4cb` (neg-фикстура). Разблокировал `[M-p200-17-remaining-1-fmtbuf]` (split fmt_buf/{core,core_test} снова можно делать — НЕ выполнено в этой волне, вне scope). | `compiler-codegen/src/imports.rs` (resolve_imports_inline_ex/resolve_one) | ✅ DONE |
| `[M-vec-of-fn-newtype-codegen]` | **OPEN P2 (волна сноса handler_fn, 2026-07-23).** `Vec[T]` при T = newtype-over-fn (ОКНО-5, D52) → codegen error «cannot infer type argument T for copy_n_nonoverlapping». Мин-репро: `type QH fn(int) -> int` + `mut v = []QH.new(); v.push(...)`. Следствие: `Middleware` оставлен record-newtype (Router.layers = []Middleware); после фикса — мигрировать Middleware на newtype-over-fn (плоская форма уже канон конструктором). | emit_c generic mono / Vec | P2 |
| `[M-closure-light-newtype-over-fn-param-misinfer]` | **OPEN P2 (волна сноса, 2026-07-23; семья №35в closure-light).** `|req| ...` в позиции, ожидающей newtype-over-fn (Handler): checker проходит, codegen эмитит параметр как `nova_int` → CC-FAIL. Обход волны: везде closure-full `fn(req ServerRequest) -> ServerResponse`. Та же семья, что закрытые-в-обход closure-light грани middleware_test/bg-tasks. | emit_c closure-light param infer | P2 |
| `[M-fn-value-ident-call-capture-field-newtype]` | **OPEN P2 (волна сноса, 2026-07-23; расширение №32а на newtype-over-fn).** Вызов по идентификатору (а) захваченного в замыкании fn/newtype-значения, (б) локала из чтения ПОЛЯ → богус `nova_fn_<имя>`. Работают: параметры (в т.ч. closure-собственные), локалы из вызовов, match-байндинги. Обход: хойст тел в top-level *_apply-функции (next параметром). Корень №32а (emit_call Ident-callee не консультирует var_types для локалей fn-типа) — диагностирован ОКНОМ-3. | emit_c Ident-callee dispatch | P2 |
| `[M-imports-order-dependent-cycle]` | ✅ **CLOSED 2026-07-20, ВЛИТО в main (297ee2651+6ec40a5cd; статус-строка «не влито» протухла, исправлена 2026-07-23). Остаточная узкая дыра multi-peer → [M-imports-multipeer-cycle-partial-exports] (221.1 Ф.2 №14, живой прецедент server↔servernet). Приоритет №1 владельца, найден Ф4R-волной (Plan 208 §10R Ш1/Ш2, `docs/plans/wip/208-f4r-notes.md`).** Родня `[M-imports-entry-folder-module-self-cycle-empty-exports]` — ТА ЖЕ схема, генерализована с entry на ЛЮБОЙ модуль. Симптом: двусторонний межмодульный цикл `A ↔ B` + третий файл `C`, импортирующий имена из ОБОИХ двумя отдельными top-level `import`-строками — итог (PASS/CODEGEN-FAIL) зависел от ТЕКСТОВОГО ПОРЯДКА этих строк (реальный прецедент: `runtime.fmt_buf ↔ runtime.string_builder`, Ф.4R Ш1 v1-архитектура). Root-cause: `resolve_one` ставит `module_key` в `in_progress` ДО цикла по peer-файлам, но `module_exports_cache` (итоговые экспорт-имена, кладутся в `visited` в конце) собирался только ПОСЛЕ рекурсивного резолва СОБСТВЕННЫХ импортов peer'а — хотя `exported_names_from_items` (уже существовала для entry-фикса) вычисляет экспорт-имена ИЗ `peer_module.items` БЕЗ резолва импортов вообще. Если во время резолва своих импортов модуль встречает обратную ссылку на себя (цикл) — cycle-guard стрелял с ПУСТЫМ `visible_acc`, даже если модуль-цели экспорты давно вычислимы. Решение — **по спеке D291** (`spec/decisions/07-modules.md`: «cross-module cycles allowed», амендит устаревший D29 rev-1 текст-остаток «циклы запрещены»; D291 также прямо требует архитектуру «collect-signatures-first, lazy bodies», которую факт. реализация нарушала): **отдавать exports** (не вводить `E_IMPORT_CYCLE` — было бы ретракцией уже принятого D291). Фикс: (1) `exported_names_from_items` поднята с local-fn (внутри `resolve_imports_inline_ex`) на top-level `pub(crate) fn` в `imports.rs`, переиспользуется; (2) в `resolve_one`, в цикле по `resolved_paths`, сразу после парсинга peer'а и ДО рекурсии в его импорты — `visited.entry(module_key).or_insert_with(Vec::new).extend(exported_names_from_items(&peer_module.items))` (провизорный кэш, растёт по peer'ам, финальный `visited.insert` в конце `resolve_one` замещает его идентичным контентом). Известное узкое ограничение (не регрессия — строго монотонное улучшение): multi-peer folder-модуль, если цикл замкнётся ДО парсинга всех peers — провизорный список неполный. **Побочный эффект (предвиден и согласован с владельцем):** existing neg-фикстура `spec_tests/conformance/entry_self_cycle/{cyc_a,cyc_b,cycle_test}.nv` кодировала СТРУКТУРНО ИДЕНТИЧНЫЙ кейс как `EXPECT_COMPILE_ERROR` (баг, зафиксированный как «защищённое поведение») — переведена в ПОЗИТИВНУЮ (`assert(a_val()==3); assert(b_calls_a()==3)`, PASS), комментарии всех 3 файлов обновлены с историей. Новая фикстура `spec_tests/conformance/order_dependent_cycle/{odc_a,odc_b,odc_c1,odc_c2}.nv` (тот же A/B, третий файл в ДВУХ порядках) — на baseline-бинаре `odc_c1` CODEGEN-FAIL / `odc_c2` PASS (флип подтверждён), на фикс-бинаре ОБА PASS. Верификация: repro (симметричный + асимметричный, matching fmt_buf/string_builder shape) — на baseline order-dependent флип воспроизведён, на фиксе — детерминированный PASS в обоих порядках; `std/src/checksums` PASS 3/0; `std/src/collections` PASS 13/0; флагман `examples/flagship/aggregator --strict-effects` built чисто; folder-CU `spec_tests/conformance` (оба лейна) — см. коммит/отчёт волны для точных чисел. Разблокировал план 208 §10R Ш2 (fmt_buf↔prelude.protocols цикл теперь либо резолвится, либо даёт честную ошибку — не порядко-зависим). | `compiler-codegen/src/imports.rs` (resolve_one) | ✅ DONE |
| `[M-fmt-write-protocol-collision-cycle-adjacent]` | ✅ **CLOSED 2026-07-21 (sonnet, worktree `nova-wpcol`, ветка `p-fix-write-collision`, НЕ влито в main — ждёт интегратора). Последний блокер Ш2 = финала 208 «один путь».** Root-cause подтверждён инструментированной трассировкой (eprintln на `types.insert`, снята перед коммитом): `TypeCheckCtx::build` (`compiler-codegen/src/types/mod.rs`) держит `types: HashMap<String, &TypeDecl>` — ГЛОБАЛЬНУЮ по ГОЛОМУ имени, без module-qualification, last-write-wins. `std.io.core` И `std.prelude.protocols` ОБА объявляют `export type Write protocol` (io.Write требует `flush()`, fmt-Write — только `@write(bytes)`) — РАЗНЫЕ протоколы, ОБА реально присутствуют в CU (io.Write подтягивается транзитивно НЕЗАВИСИМО от репро-цикла, не он его приносит). Трасса подтвердила: без цикла io.Write вставляется ПЕРВЫМ, protocols.Write — ВТОРЫМ и побеждает (корректно); с циклом (временный `import std.prelude.protocols.{Fmt}` в `fmt_buf/core.nv`) порядок ФЛИПАЕТСЯ — protocols.Write первым, io.Write вторым и побеждает → E7301 (гипотеза заметок Ш2 была неточной: НЕ дублирование ОДНОГО протокола через два пути цикла, а коллизия ДВУХ РАЗНЫХ типов, чей относительный merge-order флипает соседний цикл). Фикс (НЕ спец-ветка под fmt_buf/Write, общий механизм): (1) существующий per-file overlay `file_local_types` (ранее — только для `priv(file) type`, `[M-198-f4c-1-privfile-type-not-discriminated]`) обобщён на ЛЮБОЙ `Item::Type` (типы/mod.rs `build`, ~3636-3663) — каждый type recoverable по своему ДЕКЛАРИРУЮЩЕМУ file_id, без побочных коллизий (один файл не объявляет одно имя дважды); (2) `protocol_mismatch_found` (~14519-14556) — резолвит протокол через УЖЕ существующий `types_get_for_file(name, span.file_id)` (span — REFERRING TypeRef, e.g. `sink Write` в protocols.nv) вместо `self.types.get(name)`; (3) `protocol_missing_methods` (~14654-14695, метод-список для missing-report + `use`-embed рекурсия) получил параметр `use_file_id` (threaded через расширенный `Req::Named(String, FileId)`) — ВЕРХНИЙ вызов использует file_id call-site'а, РЕКУРСИВНЫЕ embed-вызовы (`use Write` внутри `Fmt`) переанкорятся на `td.span.file_id` (файл, где ФИЗИЧЕСКИ написан embed, а не файл исходного call-site — вторая итерация фикса, первая версия оставляла `E_NO_MATCHING_OVERLOAD` на `p.display(fmt_ctx)` из-за этого пробела). Верификация: `nova check` (checker-only, обходит ДВА НЕСВЯЗАННЫХ pre-existing бага в emit_c.rs — write_at P67-LEGACY ICE на `d216_ptr_methods_174_5.nv` ломает ЛЮБОЙ full-pipeline прогон ВСЕЙ папки conformance, и отдельный CC-FAIL на изолированных FmtCtx-репро — ОБА НЕ мои, чужая зона closure-peek/check_consume, доложено отдельно) — репро-цикл RED (`[E7301]` на d374 И `vec_f32_chained_debug.nv`, подтверждает CU-глобальность коллизии) → GREEN после фикса; 20-файловый protocol-fixture набор (d42/d53/d72/d355/d374/d142/pos_protocol_lit_×10 + neg_protocol_param_×4) — PASS 16/FAIL 4 ИДЕНТИЧНО с циклом и без (neg-фикстуры корректно FAIL — это их EXPECT_COMPILE_ERROR роль, не регрессия); `nova test std/src/checksums` PASS 3/0; `nova test std/src/collections` PASS 13/0; флагман `examples/flagship/aggregator/src/main.nv --strict-effects` built чисто (54.77s). Мега-CU (полный `spec_tests/conformance`) НЕ гонялся — заблокирован НЕСВЯЗАННЫМ write_at ICE (см. выше), задокументирован отдельно для интегратора/владельца. Репро-цикл (`import std.prelude.protocols.{Fmt}` в `fmt_buf/core.nv`) снят — временный маркер, НЕ коммитился. Полные заметки/трасса: `docs/plans/wip/write-collision-notes.md`. | `compiler-codegen/src/types/mod.rs` (TypeCheckCtx::build/protocol_mismatch_found/protocol_missing_methods) | ✅ DONE |
| `[M-write-at-p67-legacy-ice-conformance-folder]` | **ДУБЛЬ → `[M-d216-write-at-return-type-unknown-cc-panic]` (P1-секция выше; та же паника, найдена независимо двумя волнами; чинится агентом A-B8 плана 221; вести по основному маркеру).** Исходная запись: **OPEN 2026-07-21 (найден КАК ПОБОЧНЫЙ ЭФФЕКТ при верификации `[M-fmt-write-protocol-collision-cycle-adjacent]`, worktree `nova-wpcol`, НЕ моя зона — не чинил, брифом запрещено).** ЛЮБОЙ full-pipeline прогон (`nova test`/`nova-codegen test-build`) ВСЕЙ папки `spec_tests/conformance` (folder-CU, ~169 файлов) падает с ICE `internal error at emit_c.rs:53362: [P67-LEGACY] method call .write_at return type unknown — checker must annotate; obj=Ident(q)` — источник `spec_tests/conformance/d216_ptr_methods_174_5.nv:18` (`q.write_at(1, 99)`, Plan 174.5, старая стабильная фикстура). Подтверждено на НЕТРОНУТОМ дереве worktree (main @ c190de41e) — ПРЕДСУЩЕСТВУЮЩИЙ дефект, не связан с write-protocol-collision волной. Похоже на checker return-type annotation gap в зоне closure-peek (~16769)/check_consume — обеим явно запрещено трогать в брифе этой волны. Блокирует ЛЮБУЮ full-CU (codegen) верификацию всей папки conformance целиком (гейт этой волны использовал `nova check`, checker-only, который не доходит до emit_c и не задет). Отдельно: `nova-codegen test-build` на ИЗОЛИРОВАННОМ single-file repro (own module, вне spec_tests.conformance) с `FmtCtx.bare`/Display-диспетчем даёт СВОЙ CC-FAIL (`passing int to nova_str`, primitive `@display` bodies bool/char, generated C ~8826/8835/8844) — ТОЖЕ pre-existing, ТОЖЕ emit_c-стадия, ТОЖЕ не мой. Нужен отдельный компиляторный агент/волна для диагностики (вероятно связано с недавней работой в checker return-type annotation зоне — сверить с активными P67/closure-peek/check_consume волнами). | emit_c.rs / checker return-type annotation (зона closure-peek / check_consume) | **P1 (блокирует мега-CU гейт)** |
| `[M-d55-const-bytes-lit-not-constexpr]` | **ЗАКРЫТ 2026-07-21 (worktree `nova-b6tail`, ветка `p-fix-b6-tail`, sonnet).** Root cause оказался НЕ архитектурным пробелом choke-point'а, как предполагал OPEN-текст — эмпирика показала, что D429 `#coerce`-rewrite (`try_coerce_leaf`, types/mod.rs) **уже** переписывает `"hi"` → `"hi".bytes()` для module-level `Item::Const` (walk покрывает его) и корректно роутит в pre-existing `emit_lazy_const` (module-level `ro NAME = EXPR` runtime-init, `nova_consts_init()`) — но синтезированный rewrite-узел получает свежий `ExprId`, который НИКОГДА не попадает в `resolved_callees` (карта захватывается из чекера ДО AST-мутирующего прохода) — codegen-диспетч `.bytes()`-вызова падает на `resolved_callees`-канал, дальше на `method_overloads[("str","bytes")]` — который ТОЖЕ пуст на этой стадии пайплайна (module-level consts эмитятся на этапе "1b", ДО прохода, регистрирующего std `str @bytes()` в `method_overloads`) → диспетч проваливается в далёкий permissive fallback и мисрезолвит голое имя метода "bytes" в НЕСВЯЗАННЫЙ зарегистрированный символ (`bench.bytes` → `nova_bench_set_throughput_bytes`, Plan 57 bench DSL, `external_registry.rs` NAMESPACE_OVERRIDES) → `Nova_bench_static_bytes` undefined-symbol LINK FAILURE (тихий мискомпайл: чекер принимал БЕЗ диагностики, падало только на линковке). Фикс — `emit_lazy_const` (`emit_c.rs`) распознаёт ЭТУ ОДНУ конкретную, хорошо известную форму (`Call{Member{obj: StrLit(s), name:"bytes"}, args:[]}` на bytes-slice target) напрямую и эмитит корректный, уже доказанно-верный `Nova_str_method_bytes(<literal>)` сам — минуя method-registry timing gap для этого узкого, структурно-распознаваемого случая (реордер emit-пайплайна — вне объёма, широкий blast radius на каждый const в каждом CU). Scope-local `Stmt::Const` — ОТДЕЛЬНЫЙ путь: D429-walk на нём `Stmt::Const(_) => {}` (deliberate no-op, Plan 114.4 Ф.2), block-scope не имеет эквивалента `nova_consts_init()` для lazy-init → превращено в явную диагностику `[E_CONST_BYTES_NOT_CONSTEXPR]` вместо тихого мискомпайла (второй честный вариант из задания, а не первый — для module-level получилось (а), для scope-local — только (б)). Фикстуры: `spec_tests/conformance/d55_const_bytes_lit.nv` (позитив, module-level), `spec_tests/conformance/neg/d55_scope_const_bytes_neg.nv` (EXPECT_COMPILE_ERROR E_CONST_BYTES_NOT_CONSTEXPR). Гейты: оба репро RED→GREEN (изолированный dev-модуль до/после отката фикса); задетые d55/match-фикстуры (14 + 2 новых) `nova check`/`nova test` PASS; `examples/flagship/aggregator` `nova build --strict-effects` зелёный. Мега-CU `spec_tests/conformance` НЕ гонялся (за интегратором). | D55 / codegen (`emit_lazy_const`, `emit_stmt_inner`) | ✅ ЗАКРЫТ |
| `[M-slice-ext-receiver-for-in-elem-type]` | ✅ **ЗАКРЫТ 2026-07-19 (коммит 71938307a, слит e0d03c6f9 Merge p-fix-slice-ext-forin; запись была стухшей — статус-фикс 2026-07-21 по Б5-проверке 221.1: репро slice_ext_receiver_for_in_elem_ok.nv 3/3 PASS на текущем main, обход в flagship/domain.nv снят). Исходная запись: **OPEN 2026-07-18 (найден интегратором на живом коде, ФИКС-ВОЛНА ЗАПУЩЕНА тем же днём — p-fix-slice-ext-forin).** Итерация голого ресивера `for r in @` внутри slice-расширения (`fn []TaskResult @to_report`) роняет тип элемента в nova_int-fallback: `r.elapsed_ms` → CC-FAIL «member reference base type nova_int», match по `r.status` резолвит теги в ЧУЖИЕ суммы (NOVA_TAG_OnceState_Done/NetError_Cancelled вместо TaskStatus) — второе окно гадает при пустом канале. Обход-носитель: examples/.../domain/domain.nv (аннотированный биндинг `ro results []TaskResult = @`), снимается фикс-волной. Родня: [M-vec-ext-method-untyped-let-breaks-chain-dispatch] (f3_check_member_ctx). | checker types/mod.rs, for-in elem-инференс SelfAccess | **P1** |
| `[M-compound-assign-mul-div-overload-gap]` | **OPEN 2026-07-18 (найдено линт-волной 185/стиль на живом сайте).** Компаунд-присваивание диспатчит операторные перегрузки ТОЛЬКО для Add/Sub: `d = d * multiplier` на Duration (value-record с Mul-перегрузкой) компилируется, а эквивалентный `d *= multiplier` — CC-FAIL (emit_c компаунд-ветка не консультирует перегрузки для Mul/Div). Несимметричность: `a OP= e ≡ a = a OP e` (формула спеки, rejected.md §??=) нарушена для *=//= на перегруженных типах. Живой сайт: std/src/concurrency/retry.nv (лint W_NON_COMPOUND_ASSIGN сужен до +=/-= из-за этого — после фикса расширить линт обратно на *=//=). Фикс: компаунд-лоуэринг через ту же перегрузко-осведомлённую ветку, что бинарный OP (одно окно). | codegen emit_c, compound-assign | **P2** |
| `[M-d376-slow-suffix-folder-module-peer-merge]` | **OPEN 2026-07-17 (найден TLS-регресс-агентом, обойдён).** D376 `_slow.nv`-суффикс уважается discovery-walker'ом (`test_runner.rs::walk_nv_filtered_ex`), но НЕ peer-merge'ем folder-модуля: `imports.rs::resolve_imports_inline_ex` (~3073-3110) фильтрует только точный `_test`-суффикс и слепо тащит в compile-unit ЛЮБОЙ `.nv` из папки entry-файла → `_slow.nv`-тест рядом с модулем гоняется на КАЖДОМ `nova test` независимо от `--include-slow` (проверено эмпирически). Т.е. slow-lane сейчас работает только для standalone-файлов/одномодульных пакетов, НЕ для multi-peer folder-module тест-сюит. Обход-прецедент: nova-tls `src/slow/` (свой `module slow.*`, peer-scan нерекурсивен). Фикс: научить peer-scan D376-фильтру (тот же суффикс-предикат, что walker). | test_runner/imports.rs, D376 | **P2** |
| `[M-127-consume-escape-path-sensitive]` | Consume-escape analysis всё ещё V1 syntactic; path-sensitive DFG V2 deferred. | plan-127 Followups | P2 |
| `[M-73.1-comprehensive-negatives]` | Returned-view + generic-propagation (D156) consume-binding negatives отсутствуют. | plan-73.1 Followups | P2 |

## P2 — Codegen

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-interp-numeric-fallback-silent-garbage]` | **OPEN (2026-07-17, найдено при закрытии `[M-208-generic-interp-display-dispatch-gap]`, ветка `p-interp-generic-dispatch`).** `emit_interpolated_str`'s ПОСЛЕДНИЙ fallback-арм (`nova_int_to_str((nova_int)(v))`, `emit_c.rs` ~40895+) молча печатает RAW POINTER как целое число для ЛЮБОГО типа, чей dispatch (`has_explicit` / generic-mono / `try_synthesize_default_method` / `str.from` / `to_str`) не нашёл цели — не только для generic-контейнеров (тот класс закрыт этой волной через `try_generic_mono_interp_dispatch`), а для ЛЮБОГО `Nova_`-типа вообще без Display/Debug/to_str. Кандидат-фикс: честная ошибка компиляции (класс `E_BAD_FORMAT_SPEC`/`E_PTR_NO_DISPLAY`-соседей) вместо тихого мусора. **НЕ починено в этой волне** — риск: та же ветка ТАКЖЕ обслуживает как минимум один намеренно-принятый деградированный путь (см. комментарий `emit_c.rs` ~40655-40661: именованные `*T`-pointer bindings под Debug без ручного `@debug_fmt` — «Auto-derive `*T` → hex deferred to `[M-91.14-ptr-auto-derive]`» — тоже падают в ЭТОТ ЖЕ numeric-cast fallback сейчас, по дизайну как временное поведение) — ужесточение fallback'а в hard-error без полного аудита ВСЕХ типов, реально доезжающих сюда (полный `spec_tests/conformance` в этой волне не гонялся — только таргетные фикстуры), рискует внезапно сломать этот и, возможно, другие ещё не найденные легитимные call-сайты. Требует: (1) грep/аудит всех типов, реально достигающих этой ветки по всему std+conformance; (2) решение по `*T`-pointer debug-пути (`[M-91.14-ptr-auto-derive]`) ПЕРЕД тем, как ужесточать fallback. | floating | P2 |
| ~~`[M-str-primitive-static-arity-overload]`~~ | ✅ **CLOSED 2026-07-17 (worktree `p-prim-static-arity`).** Root-cause — ДВА слоя, не один: (1) `types/mod.rs::f1_check_call`'s `ExprKind::Path` арм имел безусловный `is_primitive_recv { return; }` гейт (Plan 91.8a.2, историческая защита от false-positive на НЕПОЛНОМ single-known-overload view) — он же глушил и multi-known-overload arity+compat resolved_callees-регистрацию для ЛЮБОГО примитива, хотя риск неполноты относится только к single-overload случаю; (2) codegen (`emit_c.rs` ~39114-39258) уже arity-aware (сравнивает `param_c_types.len()==arg_types.len()`), но строгий C-type-string `==` слеп к Nova-типо-эквивалентным разным сериализациям — `*u8` ro-pointee параметр рендерится `"const nova_byte*"`, а ro-биндинг аргумента (`ro buf = @ptr()`) инферится как `"nova_byte*"` (без `const`) → 0 совпадений → фолбэк в `E_UNKNOWN_STATIC_METHOD`. Маркер «арность-слепой» был неточен эмпирически (debug-трейс `NOVA_DEBUG_STATIC`), но суть та же: примитивный multi-overload resolved_callees-канал был пуст. Фикс: (1) сузил `is_primitive_recv`-гейт до ТОЛЬКО single-known-overload случая — multi (`Some(multi)`) теперь проходит ТОТ ЖЕ arity+`overload_applicability`-compat механизм, что и non-primitive receiver'ы (никакого нового кода, просто снят primitive-специфичный барьер); (2) `emit_c.rs` — channel-first lookup (`resolved_callees[call_id] → fn_span`-match) ПЕРЕД строгим string-match, мирроринг уже существующих `call_consume_arg_idxs`(~26297)/facade-instance-dispatch(~37847) паттернов; string-match остался fallback'ом (unchanged для call-сайтов без channel-хита). Rename `wrap_owned`→`str.new(buf,len)` возвращён (декл+вызов в `into_str_unchecked`, NB-комменты сняты). Верификация: `nova test std/src/checksums` CODEGEN-FAIL 6/6 → PASS 3/0; δ0 `std/src/collections/vec` (PASS 1/0) + `std/src/crypto` (PASS 5/0) зелёные; спот-грепом `.c` — 2-арг вызов уходит в `Nova_str_static_new__const_nova_byte_p_nova_int(buf, n)`, 0-арг d372-сайт — в `Nova_str_static_new()`, оба нетронуты; standalone-фикстура `spec_tests/conformance/d372_canonical/m_str_prim_static_arity_overload.nv` (into_str_unchecked round-trip, negative-control подтверждён — искажённый assert честно фейлится). | Plan 196-семья / callnorm+codegen | ✅ DONE |

| `[M-177-ifexpr-value-materialize-codegen]` | ✅ **DONE 2026-06-26** (Plan 172.1, `836befcb`). Корень УТОЧНЁН (не receiver-vs-return — `push` fluent `-> @`, легитимно `Vec*`): рассинхрон emit/infer. `infer_If` не имел fallback'а unit-доминирования `[M-codegen-fluent-tail-if-unify]`, который есть в `emit_if_expr` (R3 нарушен) → вложенный if эмитит unit, infer даёт `Vec*` → внешний result-temp `Vec*` ← unit = CC-FAIL (base64 `decode_with`). Фикс: `infer_If` зеркалит `emit_if_expr`. Гейт: §7.5 0 регресс, §0 multiset-.c IDENTICAL, фикстура `cgfix_fluent_tail_if/chain`. | Plan 172.1 (172.1-…md §9) | ✅ DONE |
| `[M-177-result-over-named-tuple-codegen]` | ✅ **RESOLVED 2026-06-26** (`b022919a`, Plan 172.1): wrapper-body с by-value late-emitted named-tuple/VR-payload → late-секция `__NOVARES_VR_TYPEDEFS__` (после struct-bodies). complex.nv re-applied (`a2d01a67`), PASS. | Plan 177 §6 (codegen) | ✅ |
| `[M-177-anon-record-in-ctor-arg-codegen]` | ✅ **RESOLVED 2026-06-26** (`c724de7a`, Plan 172.1): contextual Ok/Err-арм `emit_call` ставит `expected_record_type` вокруг emit аргумента (зеркало D55). json разблокирован; workaround не нужен. | Plan 177 §6 (codegen) | ✅ |
| `[M-180-valuerecord-err-protocol-method-mono]` | **OPEN (Plan 180 Ф.0-verify 2026-07-04).** `Result[T, <value-record>]` как return-тип PROTOCOL-метода → mono-struct объявлен, но НЕ определён → CC-FAIL `unknown type name NovaRes_..._NovaValue_<Err>`. A/B: `type DeErr value` → CC-FAIL; `type DeErr {…}` (heap) → PASS (protocol полностью impl'ится/зовётся, всё прочее идентично). Free/generic-fn `Result[_, value-record]` — OK (не protocol-контекст). **Задевает Plan 180:** `SerError`/`DeError` specced `value` (§3.7), но каждый Serializer/Deserializer-метод возвращает `Result[_, SerError/DeError]`. Обход: heap-record errors (default). Фикс: mono-enrollment `Result[_, value-record]` из protocol-сигнатур. | Plan 180 Ф.0 / mono-enrollment | P2 |
| `[M-180-valuerecord-receiver-generic-method]` | **OPEN (Plan 180 Ф.0-verify 2026-07-04).** `value`-record RECEIVER + method-level-generic метод (`fn Point @serial[S Ser](s mut S)` где `Point value`) → CC-FAIL: receiver передан by-value туда, где mono ждёт `*T` (`passing 'NovaValue_Point' to incompatible type 'NovaValue_Point *'`). Heap-record receiver → OK. **Задевает Plan 180 Ф.2:** synth `@serialize[S Serializer]` на scalar/value-record DTO упрётся. Фикс до value-record-покрытия Ф.2. | Plan 180 Ф.0 / codegen generic-method receiver | P2 |
| `[M-spec-dnum-collisions-audit]` | ✅ **DONE 2026-07-03** (все 14 разрешены: D184/291/292→D363-365; D124/125/126/138/173/174/180/185/258/298→D366-377; D277-тройная→D375/376; D239=дубль-зеркало demote). Было обнаружено: sort|uniq-d по `^## D` в spec/decisions вскрыл ~12 НАСТОЯЩИХ кросс-файловых коллизий D-номеров сверх уже-исправленной тройки: **D124, D125, D126, D138, D173, D174, D180, D185, D239, D258, D277 (×3!), D298** (напр. D298 = UDP-split 04-effects ∥ test-budget 09-tooling; D174 = sync-primitives 06-concurrency ∥ prelude-attrs 07-modules). Часть `uniq-d` — легитимные amend/legacy (D216 V2/V3, D33-LEGACY) — отфильтровать. Каждая настоящая = per-collision анализ (хронология+корневость+приёмник D366+) как для D184/D291/D292. Прецедент разрешения: этой сессии D184→D363/D292→D364/D291→D365. | spec-hygiene / owner sign-off | P2 |
| `[M-104.10-diag-pipeline-correctness]` | ✅ **CLOSED (221.1 Б1, 2026-07-21, ветка `p-fix-104-diag`).** root ложной красной диагностики — все под-баги резолвлены: LSP-сторона (`nova-lsp/compiler.rs`) — 3 бага уже RESOLVED в Ф.0.5 (import-diag-swallowed, degraded-cu-red, lsp-cmd-check-drift) + numeric-codes/hardcode-lists (см. таблицу «Plan 104.10 Ф.0.5» ниже). CLI-сторона — обнаружен и починен непочиненный двойник `degraded-cu-red` в `nova check` (`[M-104.10-cli-degraded-cu-red]`, см. ниже): `check_one_file` полностью пропускал import/prelude-резолв при недоступном CWD-repo (в т.ч. игнорируя `NOVA_STD_PATH`), ложно краснея prelude-символы для standalone-файлов. Формальный spec-closeout (D-блоки) по-прежнему деферится в Ф.14 (не language-changing — только diag-пайплайн, D-амендмент не требуется). | Plan 104.10 Ф.0.5 / 221.1 Б1 | ✅ DONE |
| `[M-174.6-ffi-struct-layout]` | **OPEN (переоценка 2026-07-04, Plan 174.6 M2/M3).** S8 layout-guard `_Static_assert(sizeof(NovaValue_X)==<expected>)` для by-value FFI value-record'ов НЕ закрыт: корректный `<expected>` требует независимой C-ABI layout-модели (паддинг/выравнивание), которой у Nova нет. `sizeof==sum-полей` неверно (отвергает паддинг); `sizeof==sizeof` тавтология (для СВОЕЙ структуры C даёт тот же размер; дрейф — только против ВНЕШНЕЙ C-либы). Значит static-assert **coupled** к полной layout-спеке → закрывается вместе с ней. Обоснование — 174.6 §11. | Plan 174.6 §11 | P2 |
| `[M-174.6-ffi-abi]` | **OPEN-остаток (Plan 174.6 M2/M3, 2026-07-04).** M0/M1/M2-additive закрыты (тип-лист D282 rule 2, `*extern "C" fn` D353, checker+коэрция+conformance `d282_ffi_abi` pos+9neg+cookbook+cast-матрица D353+non-extern-C позиции M2). Остаток: полная D216 §10 ретракция строки «default C ABI» у bare `*fn` (= Nova-ABI; отложено — меняет дефолтный ABI `*fn`, reconcile-риск) + реинтерпрет `*fn`↔`*extern "C" fn` (unsafe-hatch) + codegen `fn→fn-ptr` value (P67-LEGACY). | Plan 174.6 §11 | P2 |
| `[M-174.6-varargs]` | C-varargs (`...`) в `extern "C" fn` сигнатурах не поддержаны (`printf`-семейство). Маркер из зонта 174 §3.6. | Plan 174.6 | P3 |
| `[M-spec-nova-lsp-conformance-audit]` | **OPEN (3-агентный аудит 2026-07-03):** nova-lsp не соответствует compiler-conventions — 26 находок. Критические (root красной IDE-диагностики, БЕЗ маркеров): `compiler.rs:148` молча глотает import-ошибки (§4); `compiler.rs:137` degraded-CU → пустой `peer_files` → ложная краснота prelude/peer-символов; LSP↔cmd_check дрейф (нет `collect_all_signatures`/sig_table — Plan 162.2). + §3 хардкод-списки (`prelude_items`/`STD_MODULES` со стейл-путями/`known_stdlib_*`); `diagnostic_mapping` теряет числовые `[Ennnn]`; `symbol.rs` тип `"float"`. Детали+суб-маркеры → [Plan 104.10 §0.4](104.10-lsp-v2-production.md). Бóльшая часть type-channel/rename/method-dot уже в 104.10 (Ф.2/Ф.5/Ф.7). | Plan 104.10 §0.4 | P2 |
| `[M-net-merge-to-single-effect]` | ✅ **DONE 2026-07-04 (Plan 178 Ф.0.5).** `TcpNet`/`UdpNet`/`DnsNet` → единый `Net` (реконсиляция к D62-канону) + один `real_net()`/`mock_net()`; **`AddrNet` РЕТРАКТИРОВАН** → addr-ops pure `.nv` над `extern "C"` (FFI≠эффект). `bind`-коллизия → `tcp_bind`/`udp_bind`. Мигрированы все call-sites (nova_tests plan91_12/15/16). Разблокирует 176 Ф.4(b). Byte-surface (`read_bytes`/`write_bytes`/…) приземлён тем же заходом (public-wrappers, не effect-ops). **Остаётся отдельно:** SocketAddr value-record rep (`[M-net-socketaddr-value-record]`) + полная `str`→`[]u8` net-миграция (owner-approved sweep, не блокирует). Гейт: conformance 38/38; 26/29 net-тестов PASS (флак+1 codegen-gap). | Plan 178 §13.2 / spec D62 | ✅ DONE |
| `[M-net-socketaddr-value-record]` | **OPEN (Plan 178 Ф.0.5, 2026-07-04):** `SocketAddr value { priv handle CSocketAddr }` → Nova value-record `{family, addr []u8, port u16}` (nv-sourcing, как Rust `std::net::SocketAddr`); pure-ops станут чистой Nova БЕЗ FFI; Nova↔C `sockaddr` конверсия только на границе `connect`/`bind`/`recv_from`. Требует NEW C-FFI (byte↔sockaddr обе стороны) + Nova IPv6-форматирование (`::1` compression)/parse + обновить `d364_value_record_handle_wrap.nv`. **НЕ блокирует HTTP** (HTTP берёт SocketAddr как данные, работает при любом rep — план §3.10/§13.2); byte-baseline-guarded, едет с `str`→`[]u8` sweep. Handle-rep сохранён в Ф.0.5. | Plan 178 §3.10/§13.2 | P2 |
| `[M-effect-forbid-generic-bound]` | **OPEN (зарегистрировано 2026-07-03, исследование 176 Q15):** `check_callee_effects` (types/mod.rs ~:15960, снимок) ищет callee по имени в method_table — для generic-ресивера `[W io.Write]` записи нет → **`forbid`/D63, realtime/D64 и effect-surface НЕ видят эффекты через generic protocol-bound** (`forbid Fs { body.copy_to(file_writer) }` не ловится статически). Плюс: vtable-dispatch effectful protocol-методов запрещён D122 (mono-only) — на erased-пути нужна диагностика. | 176 Q15 / spec Q6 (effect-polymorphism) | P2 |
| `[M-126-sum-equal-rich]` / `[M-126-sum-clone-rich]` / `[M-126-sum-hash-rich]` | **OPEN** (зарегистрировано Ред.2 2026-07-03; ранее только в auto-derive-guide.md:254-256): sum-auto-derive в `protocols/auto_derive.rs` — заглушки (sum-equal = identity-fallback, sum-hash = literal 0, sum-clone = self placeholder); rich per-variant synth НЕ существует. 🔴 HARD-PREREQ Plan 180 Ф.2-sum/180.2 (serde sum-derive). | auto-derive-guide §followups / Plan 180 §4 | P1 |
| `[M-net-payload-variant-static-lowering]` | **OPEN (Plan 178 Ф.0.5, 2026-07-04):** payload-variant конструктор в Path/Member-форме (`NetError.IoError(msg)`) МОЖЕТ mis-lower'иться в **undefined `_static_`-wrapper** (`Nova_NetError_static_IoError`) вместо `nova_make_NetError_IoError` — link-fail `undefined symbol`. Call-graph-зависимо: `real_net()`+str↔[]u8-конверсии (`str.from_bytes_unchecked`/`.to_bytes`) в одном CU «отравляют» классификацию варианта для всего CU (checker не аннотирует → emission падает на static-method-путь). `tcp_echo_slow` (real_net+str) линкуется, `net_byte_surface_slow` (real_net+byte) — нет. **Того же семейства, что починенный `[P67-LEGACY]`-ICE** (type-resolution, emit_c.rs 39997/43401 — я добавил sum-variant fallback); тут — сторона EMISSION (call-lowering). Fix: в call-emission редиректить `Nova_{Sum}_static_{Variant}`→`nova_make_{Sum}_{Variant}` (emit_sum_type всегда эмитит nova_make_). Blocked: real-socket byte-round-trip тест (mock-тест валидирует поверхность). | Plan 178 Ф.0.5 / Plan 172 (type-engine) | P2 |
| `[M-closure-trailing-scalar-coercion-no-typecheck]` | **✅ CLOSED 2026-07-10** (ветка destructure-lint, sonnet, решение владельца 2026-07-10). Найдено при расследовании `[M-toml-repeated-fail-call-run-fail]` (`std/encoding/toml.nv`'s `is_bare_key_char`): ни checker, ни codegen не отвергали fn-тело, чьё trailing/return-выражение — closure-литерал (`|| body` / `|x| body`), против скалярного (`bool`/int-family/float/`char`) return-типа — closure лоуэрится в fn-pointer, коэрсия в скаляр молча бит-реинтерпретирует указатель (`bool` всегда `true`), без диагностики. Fix: новая проверка `TypeCheckCtx::check_closure_scalar_return` + `_in_block`/`_in_stmt`/`_in_expr` (compiler-codegen/src/types/mod.rs) — покрывает implicit trailing (блок/arrow-body) И каждый explicit `return X` (в т.ч. вложенный в if/match/while/for/loop; НЕ в detach/spawn/parallel-for/nested-closures — другой execution-context), зеркалит reachable-позиции существующего `materialize_returns_in_block`/`_in_expr`. Новый код `E_CLOSURE_SCALAR_RETURN`, спек-амендмент [D417](../../spec/decisions/02-types.md) (02-types.md). Побочный улов той же волной прогоном по std: та же дыра (ведущий `||` на continuation-строке) ещё в `std/data/semver.nv` (`is_ascii_ident_char`) и `std/encoding/csv.nv` (`needs_quoting`) — обе ранее ВСЕГДА возвращали `true`; смигрированы на трейлинг-`||` (канон `toml.nv`). Гейты: `nova check std` δ0 (после миграции 2 найденных бага), conformance 90/0, `err173` 1/0, cargo build чист. | checker / compiler-codegen/src/types | ЗАКРЫТО |
| `[M-net-merge-focus-stub-codegen]` | **OPEN (Plan 178 Ф.0.5, 2026-07-04):** diverging (`panic`) тело effect-op'а, возвращающего value-struct/tuple (`split_stream -> (TcpReadHalf, TcpWriteHalf)`), эмитит spurious `return (nova_int)0` → CC-FAIL `returning nova_int from incompatible result type`. Инфра дивергенции есть (`emit_divergent_with_target_125` в `emit_expr_with_target_type`), но НЕ применяется к телам op-хендлеров (op-return-C-type не используется как divergent-target). Следствие merge'а: user-код не может писать partial `Net`-хендлер для focus-теста (opaque handles + `split_stream`-tuple → panic-стаб miscodegen). Fix: применить op-return-type как target в emission тела хендлера. | Plan 178 Ф.0.5 / Plan 172 (type-engine) | P3 |
| `[M-126-sum-equal-rich]` / `[M-126-sum-clone-rich]` / `[M-126-sum-hash-rich]` | **OPEN** (зарегистрировано Ред.2 2026-07-03; ранее только в auto-derive-guide.md:254-256): sum-auto-derive в `protocols/auto_derive.rs` — заглушки (sum-equal = identity-fallback, sum-hash = literal 0, sum-clone = self placeholder); rich per-variant synth НЕ существует. 🔴 HARD-PREREQ Plan 180 Ф.2-sum/180.2 (serde sum-derive). **✅ Verified OPEN эмпирически 2026-07-04** (Plan 180 Ф.0, auto_derive.rs:554-845). | auto-derive-guide §followups / Plan 180 §4 | P1 |
| `[M-180-f64-shortest-roundtrip]` | ✅ **CLOSED (Plan 180 keystone-фикс 2026-07-04).** Реализован shortest-round-trip форматтер — единая точка `nova_rt.h::nova_f64_shortest`/`nova_f32_shortest` (§0/§3): default `%g` пробуется первым (zero-churn на ≤6-знач-значениях, вкл. `100000`/`0.1`), затем эскалация точности 7..17 (f64) / 7..9 (f32) до ПЕРВОГО точного `strtod`/`strtof` round-trip. Funnel всех float→str: `str.from`, `@display`/`@debug`, `${x}`, `StringBuilder.append`, `println(float)` (`nova_print_f64/f32` теперь через тот же формат — устранён `%g`-vs-interp дрейф). Побочно: (a) `str.from(f32)` dispatch добавлен в emit_c.rs (был truncate-to-int); (b) f32 `@display`/`@debug` (protocols.nv:617) + `StringBuilder.@append(f32)` переведены с `str.from(@ as f64)` (surface widening-tail) на f32-precise `str.from(@)`. Гейты: conformance 38/0; round-trip pos-тест (8 блоков π/e/1234567.89/1e300/1e-300/denormal/2^53/-0.0 → PASS); `decode(encode(v))==v` на float JSON PASS (был RED); zero-churn на ≤6-знач-корпусе (все существующие float→str assert'ы используют ≤6 знач.цифр — эмпирически 0 изменений). | Plan 180 Ф.0 / std/encoding/json + runtime conv.h/nova_rt.h | ✅ DONE |
| `[M-option-eq-some-literal-elem-adapt]` | **OPEN 2026-06-26 (Plan 172.2).** `Option[u32] == Some(int_literal)` не адаптирует int-литерал внутри `Some` к element-типу ожидаемого Option → структурный eq генерит `NovaOpt_nova_int` где ожидается `NovaOpt_uint32_t` → CC-FAIL (`passing 'NovaOpt_nova_int' to incompatible type`). Т.е. `v.get(0) == Some(5)` валиден для `Vec[int]`, но не для `Vec[u32]`. Литерал в `Some(..)`-арг должен брать тип из ожидаемого `Option[T]` (как прямой target-typed литерал). Обойдено в POS-фикстуре `u172_2_method_arg_narrowing_pos` через `v[i]` index + mixed-compare. | Plan 172.1 (type-engine) / floating | P2 |
| `[M-172.4-rvo-move-on-last-use]` | **Перф (НЕ корректность).** Copy-elision для value-типов: свежие/уникальные value-значения (return/литерал/временное, escape-доказанные без второго владельца) строятся прямо в слоте назначения (RVO/NRVO, скрытый `sret`) либо move-on-last-use — вместо `memcpy`. Прозрачно (value: copy≡move наблюдательно; heap moot, by-ref). БЕЗ синтаксиса (вывод, как Rust/Swift; не `tmp`/`T&&`). Едет на by-value/`consume`/`return`. Ортогонально `consume` (владение ≠ copy-elision). Обобщает D326 R7/R8 (`-> @` decay). GC проще C++ (нет move-ctor/moved-from). Делать после 172.4 Ф.3-Ф.5. | Plan 172.4 Ф.6 | P3 |
| `[M-cancellation-test-mono-recursion-overflow]` → renamed `[M-mn-worker-fiber-closure-call-stack-overflow]` | ✅ **CLOSED (Plan 151, 2026-06-13).** Имя `mono-recursion` оказалось **мисдиагнозом** (Audits 2/3 Plan 149). Plan 151 Ф.0 5-way isolation matrix доказал: codegen ЧИСТ (тот же бинарь PASS под `NOVA_AUTOARM=0` / `NOVA_MAXPROCS<=3`); это НЕ мономорфизация и НЕ stack-size. **Реальный root-cause — GC-reachability bug в M:N рантайме:** heap-замыкание, передаваемое в `supervised{spawn{ ro r=body() }}`, укоренено ТОЛЬКО на native-стеке главного потока, пока main блокирован в `nova_supervised_run_impl`; при ≥4 worker'ах GC срабатывает во время `_materialize_pool` (до создания ленивой fiber-арены main'а), Boehm STW не видит main-стек → premature collect замыкания → `closure->fn` зануляется реюзом → worker-fiber зовёт NULL (RIP=0), что arena-VEH рапортует как обманчивое «fiber stack overflow in slot 0». Подтверждено: `GC_DONT_GC=1` чинит; spawn-diag показал `closure->fn==0` на `mco_resume`. **Фикс (Plan 151 Ф.1, RUNTIME — НЕ codegen):** `fiber_arena_win.c` пушит native-стек главного потока как GC-root в `_nova_fw_gc_push_other_roots` (`nova_fiber_arena_set_main_stack`, вызывается из `_materialize_pool`). cancellation_test un-quarantined (Ф.2), PASS armed 80/80 (MAXPROCS default/2/8/16); regression-guard `mn_closure_spawn_gcroot_test.nv` (Ф.4) genuine (8/8 fail pre-fix). 0 regressions (concurrency 112 PASS / 4 pre-existing). | [Plan 151](151-codegen-mono-recursion-closure-generics.md) | ✅ DONE |
| `[M-mn-gc-root-unified-stack-registry]` | Hardening (Go-`allg`-inspired): Plan 151 пропатчил КОНКРЕТНУЮ дыру (main-стек не сканировался GC до ленивой арены). Go-инвариант сильнее: ВСЕ root-bearing стеки (main + каждая worker-арена) в едином глобальном реестре, сканируемом атомарно ДО любого GC — структурно убивает будущие timing-варианты этого класса (пропущенный стек в др. окне). НЕ срочно (точечный фикс работает, 2-го экземпляра нет). **Делать с [Plan 144](144-precise-gc-implementation.md) (precise GC субсумирует — точные роуты вместо «просканировал ли все стеки»), либо on-demand.** Конкретный адрес после декомпозиции 2026-06-13: **Plan 144 §8 Ф.0** (спроектировать unified roots registry) + **Ф.3** (precise root-scan через цепочку + реестр заменяет консервативный скан). | Plan 144 §8 Ф.0/Ф.3 | P3 |
| `[M-128.1-array-namedtuple-ro-method]` | `vs[i].ro_method()` на `[]NamedTuple`: pointer-cast в int-слот vs by-value receiver → clang mismatch; gated. | plan-128 Followups | P2 |
| `[M-128.1-nonpure-index-key]` | Side-effecting `arr[next_idx()]` на pointer-ABI receiver вычисляется дважды; hoist-to-temp V2 не сделан. | plan-128 Followups | P2 |
| `[M-codegen-var-types-fn-scope]` | `var_types` (codegen local-type map) НЕ scoped по функциям — локалы протекают между функциями. Plan 139.2 surfaced: Nova-body str-метод с `Vec[u8]`-локалом `a` протёк → block-expr `{ro a=…; a+b}` мис-инферил value-тип как Vec-view → SEGV. Точечно закрыт в block-expr inference (emit_block_expr + infer Block-арм пред-регистрируют блок-локалы, commit 3917d17c); корневой fix — per-fn scope/clear var_types (broad, regression-риск). **NEW REPRO (vec-sweep, 2026-07-06):** local variable named `buf` в `std/runtime/write_buffer.nv::new()`/`@cap()` мис-типизировал `wb.len()` как `str.len()` (E_STR_NO_LEN) в НЕСВЯЗАННОМ caller-файле той же CU; переименование локали (`_wb_buf`) обошло. Тот же класс, другой surface (не block-expr — top-level fn locals). | plan-139.2 post-close | P2 |
| `[M-vec-spelling-array-value-position-cap-collision]` | ✅ **FIXED (2026-07-07, ветка cg-three-fix, emit_c.rs).** Корень: вызов метода на legacy `NovaArray_<elem>`-ресивере (C-тип, в который выводится `[]T.new()` для примитивного элемента, D38) НЕ имел записи в `generic_type_instance_info` под своим написанием → весь overload-resolving generic-instance dispatch (блок 5b) пропускался, вызов проваливался в coarse name-keyed `method_receivers` (last-wins) → `[]u8.new().cap(n)` мис-роутился на `@cap` несвязанного ко-компилируемого типа. `Nova_Vec____<elem>*` (typed-local написание) диспетчился верно — это и эксплуатировали обходы. Фикс: в блоке 5b remap `NovaArray_<elem>` → layout-идентичный `Vec____<elem>` mono-ключ (консервативно — только если инстанс Vec[<elem>] уже зарегистрирован; +каст ресивера). Обходы в write_buffer.nv/string_builder.nv СНЯТЫ. Родственный `[M-vec-spelling-same-name-method-dispatch-collision]` (field `@buf.cap(n)`) закрыт тем же. Регресс-гард: `spec_tests/conformance/dispatch_receiver_type_vs_name.nv`. **Доп. находка (2026-07-10, std-hygiene):** ещё 4 стейл-обхода этого же (уже мёртвого) бага пережили фикс необнаруженными — комментарии-предостережения «`.new().cap(n)` мисроутит на WriteBuffer.cap» в http/client/wire.nv, http/server/wire.nv (оба сняты вместе со сносом дублирующей `slice()`-обёртки), http/server/server.nv, http/servernet/servernet.nv, http/client/decompress_br_test.nv — репро чисто прошло, комментарии снесены, сайты переписаны цепочкой `[]u8.new().cap(n)`. | codegen dispatch (block 5b) | ✅ DONE |
| `[M-vec-spelling-consume-chain-cap-collision]` | ✅ **CLOSED (форс-фикс, sonnet, 2026-07-21, worktree `nova-vecspell`, ветка `p-fix-vec-spelling`).** Репро восстановлено (`consume sb = StringBuilder.new().cap(n)` внутри `std/src/runtime/string/transform.nv::@replace`) — RED подтверждён на неизменённом компиляторе. Корень: `infer_value_type` (`compiler-codegen/src/types/mod.rs:28491`, единственный резолвер типа RHS для `consume`-биндинга, ОТДЕЛЬНЫЙ от основного type-checker'а) в ветке `Call{func: Member{obj, method}}` резолвит тип ресивера ТОЛЬКО когда `obj` — простой `Ident`/`self`/`SelfAccess`; для 2-звенного (и глубже) чейна `obj` сам является `Call`-узлом (`StringBuilder.new()`) → падало в `_ => None` → тип ВСЕЙ цепочки не резолвился → `var_types["sb"]` не заполнялся → `is_consume_method("sb","into_str")` всегда `false` → `sb` никогда не помечался Consumed → ложный `[D133-not-consumed] (тип ``)` на scope-exit. Диагноз «ломает ВСЕ ОСТАЛЬНЫЕ сайты в CU» из исходной записи не подтвердился в узком репро (падал именно и только сам chain-биндинг); возможно относился к более сложному CU-контексту оригинального обнаружения, не переисследовано отдельно. Фикс: рекурсивный fallback `_ => self.infer_value_type(obj)` в этой ветке — резолвит receiver любой глубины чейна (базовый `Type.new(...)`-Call уже обрабатывается первой веткой той же функции). Канонизированы 3 std-сайта-обхода (сняты комментарии-пины) на `T.new(cap: n)`: `std/src/runtime/string/transform.nv` (`@replace`, `@replacen`), `std/src/unicode/cp_utils.nv` (`cps_to_str`). Гейты: repro RED→GREEN (`nova check std/src/runtime/string`); `nova check std/src/unicode` PASS; `string_builder_test`/`fmt_buf/core` PASS; consume-регресс d133/d157/d180/d164/d174/d179/d86 (10 фикстур) 10/10 PASS; полный `nova check std --strict-effects` (одна CU) — 142 PASS / 17 FAIL / 1040 WARN, БЕЗ РЕГРЕССИЙ (идентичный tally на неизменённом main-компиляторе — 17 FAIL преэкзистентны, не связаны); флагман `examples/flagship/aggregator` собран (`built: main.exe`, только преэкзистентные warnings). | consume-checker (`types/mod.rs::infer_value_type`, `ExprKind::Member` арм) | ✅ DONE |
| `[M-vec-spelling-maplit-desugar-cap-ice]` → уточнён: `[M-maplit-folder-cu-insert-new-ice]` | **PRE-EXISTING НА MAIN — ДОКАЗАНО (ремонт d102, 2026-07-07).** ICE `"method call `.insert_new` return type unknown"` (P67-LEGACY, emit_c.rs:42872 в нумерации main) при компиляции `nova_tests/map_literals/` folder-module воспроизводится с БАЙТ-ИДЕНТИЧНЫМ main компилятором + std/nova_tests ДО каких-либо правок этой ветки (изоляционная сборка: `git checkout main -- compiler-codegen/src/{desugar,ast/mod,types/mod,codegen/emit_c}.rs` + `git checkout 58d0d535 -- std nova_tests` → rebuild → тот же ICE). **РЕПРОДУКЦИЯ:** `nova test nova_tests/map_literals` (folder-module CU, ≥2 файлов вместе; первым выдаётся посторонний `E_CONST_NOT_CONSTEXPR` с ложной атрибуцией на import-строку positive_clone_merge.nv, затем ICE на `_m0.insert_new` из десугаренного map-литерала). Одиночный файл в изоляции ICE НЕ даёт. Моя ранняя атрибуция «ICE от добавленного `.cap(n)` pre-sizing statement» была ОШИБКОЙ — ICE не зависит от cap-statement вовсе. Pre-sizing из map-literal desugar'а всё равно снят (см. desugar.rs) как принятая perf-only мера при удалении `with_capacity`; возврат pre-sizing возможен после фикса этого ICE. По §4а корень чинится отдельным заходом. | codegen/checker (folder-CU annotation drift) | P1 |
| `[M-vec-spelling-hashmap-buckets-array-gap]` | **OPEN, RE-VERIFIED (vec-sweep, 2026-07-06).** `std/collections/hashmap.nv::new_buckets` строит bucket-массив ЯВНО через `Vec[Slot[K,V]]` (не `[]Slot[K,V]`) — Ф.0b workaround из Plan 138.2, датированный ДО универсального `[]T`-флипа. Перепроверено заново в этом заходе: замена на `[]Slot[K,V]` + `.new().cap(n)` даёт RUN-FAIL (rehash-тесты ломаются на рантайме). Тот же класс, что `[M-153.x-array-new-not-vec]`, всё ещё жив — `Vec[...]` остаётся санкционированным исключением из `[]T`-канона в этом конкретном месте (маркер в коде на месте). | Plan 138.2 / codegen array-erasure | P2 |
| `[M-vec-new-static-arity-overload]` | ✅ **CLOSED (форс-фикс, sonnet, 2026-07-12, worktree `nova-capmig`, Plan 200 П5).** Ред. 2026-07-06 «РАБОТАЕТ, риск не материализовался» ниже — ОШИБОЧНАЯ (одиночный-файл прогон не воспроизводил дефект); тот же день regression был найден при сборке ВСЕГО vec-folder-модуля одной CU (`vec_of_empty_panic` CC-FAIL) и `from_raw_parts` временно откачен (см. history `std/collections/vec/core.nv`). Реальный root cause — ДВА co-located name-only (arity-blind) overload-резолва, оба «первый по имени», игнорирующие арность: (1) `compiler-codegen/src/callnorm.rs` `Sigs::static_methods` фильтровал прочь `(type,method)` с >1 сигнатурой (default-arg backfill пропускался для overloaded ctor) — фикс хранит ВСЕ overload'ы + `pick_static_params` дизамбигуирует по `bind_call_args`; (2) `compiler-codegen/src/codegen/emit_c.rs` ветка «1b» (turbofish static-ctor call, ~emit_call 32577) резолвила `generic_type_methods[base].find(name)` первым совпадением — портирована схема соседней ветки «5b» (`[M-138.2-generic-method-overload-mono]`: арность → param-C-type → `resolved_callees`-span чекера + per-overload `__<paramtype>` mono-suffix). Фолд `from_raw_parts` → `Vec[T].new(ptr *mut T, len int, cap int) -> Self requires len >= 0 && cap >= len => { data: ptr, len, cap }` сделан попутно; единственный call-site (`str @bytes()`) переведён. Гейты: `vec_of_empty_panic` зелёный, `nova test --full std/collections/vec` (folder-CU, 2/2) + `std/collections`+`std/checksums`+`std/crypto` (21/21) без cross-wiring, conformance single-CU 95/0. | vec-sweep / codegen static-overload (callnorm.rs + emit_c.rs branch «1b») | ✅ DONE |
| `[M-vec-spelling-lint]` | **TODO (vec-sweep, 2026-07-06).** Checker-lint `W_VEC_SPELLING`: предупреждать на `Vec[T]` вне `std/collections/vec/` (definition-site) без сопровождающего маркера-исключения в комментарии — держит канон `[]T` (D239 amend) от тихого регресса. Не реализовано в этом заходе (нет времени); нужен отдельный проход. | vec-sweep follow-up | P3 |
| `[M-vec-spelling-http-after-props]` | **TODO.** `std/http/` целиком исключён из `[]T`/`with_capacity`→`cap()` sweep этого захода (параллельный агент работает над http-props в то же время). Нужен отдельный проход ПОСЛЕ слияния http-props: `[]T`-канонизация + `with_capacity`→`new().cap(n)` (`std/http/client/*`, `std/http/server/*`, `std/http/*.nv` — ~15 файлов с `.with_capacity(`). | vec-sweep follow-up / std/http | P2 |
| ~~`[M-vec-neg-panic-cascaded-cc-fail]`~~ | ✅ **RESOLVED (ремонт d102, 2026-07-07).** Корень НАЙДЕН и починен: НЕ каскад от access.nv — это был `StringBuilder @clone` этой же ветки: тело `{ buf: sb_init_buf(...) }.append(@buf)` на record-literal-ресивере мис-диспатчило `append` в str-перегрузку (`Nova_StringBuilder_method_append(sb, Vec*)` при параметре `nova_str`). Фикс: helper `sb_clone_buf(src []u8) -> []u8` (append на типизированной `[]u8`-локали), `@clone => { buf: sb_clone_buf(@buf) }`. `vec_of_empty_panic` теперь PASS; `--full std/collections/vec` дельта = только известный pre-existing `access.nv` e7320 ([M-vec-access-e7320-as-bytes-str]). Побочный урок (класс бага): overload-диспатч по имени на record-literal receiver не учитывает тип аргумента — родственно [M-method-resolution-registry-inconsistency]. | vec-sweep / codegen | ✅ DONE |
| `[M-vec-spelling-static-dispatch-gap]` (partial close) | **ЧАСТИЧНО ЗАКРЫТ (ремонт d102, 2026-07-07).** `[]T.of(...)`/`[]T.from(...)`/любой Vec-static на `[]T`-алиасе теперь диспатчится корректно: D38-блок emit_call реврайтит ресивер `Path(["__array",T])` в `Vec[T]`-TurboFish и повторно входит в emit_call (variadic-упаковка `of`, mono-регистрация — вся существующая Vec-машинерия); infer_expr_c_type зеркалит (R3). ЭТО и был корень «регрессии d102»: миграция предшественника `Vec[int].of`→`[]int.of` в d259 (тот же folder-CU) падала на этом pre-existing гэпе с ложной атрибуцией ошибки на d102 (доказано изоляционной сборкой main-компилятора). **Остаток гэпа:** `new`/`with_capacity` на `[]T` НАМЕРЕННО оставлены на legacy erased-пути (байт-идентичность main) = [M-153.x-array-new-not-vec]; их результат без явной `[]T`-аннотации биндинга ломает диспатч rich-Vec-методов (обход: аннотация; см. d232-исключение). | vec-sweep / codegen | P2 (остаток) |
| `[M-vec-spelling-nova-tests-after-182]` | **TODO (частично сделано, vec-sweep 2026-07-06).** `nova_tests/` НЕ мигрируется на `[]T`-канон целиком (маркер существовал до этого захода — общая политика: nova_tests не трогаем массово до Plan 182). В ЭТОМ заходе точечно мигрированы `with_capacity`/`from_raw_parts` call-sites, которые СЛОМАЛИСЬ бы после удаления этих методов (~40 файлов обработано). Остаток (не проверено/не мигрировано в этом заходе, все НЕ входят в гейты): `nova_tests/strings/str_builder_constructors.nv`, `str_builder_metrics.nv`, `nova_tests/plan55/f5_hashmap_infer_no_annot.nv`, `plan91_fe1/{composite_array_map_pos,method_turbofish_pos}.nv`, `plan91_fe1/neg/composite_array_edge.nv`, `plan11_followup/{f10,f19}...positive.nv`, `plan138_2/t6_fluent_push.nv`, `plan145_2/composite_get_option_pos.nv`, `plan73/binding_*.nv` (частично), `plan91_7/array_new_test.nv`, `self_nested/repro_control.nv` (частично), `map_literals/{positive_empty_in_map_position,positive_insert_new_correctness,positive_int_int}.nv` (нужна verify-прогонка после map-literal desugar изменений). | vec-sweep follow-up / nova_tests после Plan 182 | P3 |
| `[M-138.2-self-in-param]` | `Self` в param-позиции generic-метода (`@append(other Self)`, `@copy_from`/`@compare`/`@equal`) мис-лоуэрит C-тип без receiver-subst → forward-decl≠def; workaround явный `Vec[T]`. (NB Ф.0d: `@append` теперь COPY, signature `@append(other Vec[T])` без `mut` — D141.) **🟡 ЧАСТИЧНО ЗАКРЫТО (расширено 2026-06-15, branch `plan-self-nested-generic`, commits `60a875a0`+`9f614e9b`).** ✅ **`Self` как ВЛОЖЕННЫЙ type-arg в Named generic** на mono-инстансе value-generic — FIXED в ОБЕИХ позициях: RETURN (`-> MapIter[Self, U, V]`) И PARAM (`g FilterIter[Self, U]`). Root: call-site return-inference биндил `Self` value-aware (без trailing `*`), но `register_mono_method_instance` (fwd-decl, ~13946) + `emit_monomorphized_method` (body, ~14030) строили `current_type_subst` только из receiver-generics (без `Self`-записи) → вложенный `Self` промахивался мимо early-lookup (`type_ref_to_c:5265`) и падал в `"Self"`-арм → POINTER-форма → spurious trailing `*`→`_p` в mangle → расходящееся C-имя mono. Fix: в обоих местах после `current_receiver_type` биндить `Self`→`value_aware_generic_c_type("Nova_{recv}*")` в `current_type_subst` через `.entry().or_insert()` (no-clobber), guard `recv_type.contains("____")` (только mono-инстансы; `value_aware_*` оставляет heap-generic/non-value формы без изменений → top-level heap `-> Self` не затронут). Verified в сген. C: fwd-decl/call-site/body одного метода делят один mono; spurious `_p`=0. Regression-guard `nova_tests/self_nested/` 6/6: 4 поз./контроль (repro=nested-RETURN, repro_param=nested-PARAM, repro_explicit=control, repro_control=heap-generic top-level `-> Self`-chains) + 2 негатива `EXPECT_COMPILE_ERROR` (`self_no_receiver_fail`=голый `Self` без receiver → `[E7001] Self type used outside receiver`; `self_nested_arity_fail`=`MapIt[Self]` неверная арность host → `[E7310] expects 3 type arguments`). Stdlib: `std/collections/vec_iter_zc.nv` 8 adapter-on-adapter методов переведены на `Self` (`refactor` 9f614e9b). Spec — [02-types.md → D66 §«Self как вложенный generic type-arg»](../../spec/decisions/02-types.md#d66-self-universal--ссылка-на-обобщающий-тип-в-методах-effects-protocols). План/критерии приёмки (H6, вкл. обязательный «без упрощений как для прода») — [138.4 Addendum Ф.6/G-E](138.4-generic-method-codegen-hardening.md#addendum-ф6--g-e-self-как-вложенный-generic-type-arg-2026-06-15). **ОСТАЁТСЯ (вне покрытия фикса):** (1) `Self` == single-param generic (`VecIter[T]`), использованный как type-arg ВНУТРИ multi-param адаптера (`MapIter`) — chain-ENTRY методы `VecIter[T] @zmap -> MapIter[Self,T,U]` ломают codegen (`E_PRIMITIVE_NO_PROTOCOL_METHOD` — receiver мис-резолвится в `int`); потому VecIter-entry оставлены explicit (latent compiler limitation). (2) top-level `other Self` param на ERASED-stub heap-generic (fwd-decl `Nova_Stack*` vs body `nova_str` — pre-existing erased-`Self`-resolution, отд. code-path `emit_generic_method_erased`/`emit_generic_static_method_stub`) + generic-static-method param-subst (`Vec[T].from` `[]T`-stub, 02-types.md §5). | plan-138 Followups | P2 |
| `[M-153-vec-of-variadic-codegen]` | ✅ **CLOSED 2026-06-14.** Эргономичный variadic-конструктор `export fn Vec[T].of(...args []T) -> Self => args` залендён в `std/collections/vec/core.nv` (заменил DEFERRED-NOTE). **Root-cause был узким:** `lookup_variadic_arity` (emit_c.rs `recv_method`-match) НЕ распознавал turbofish-static-форму `Type[T].method(...)` = `Member{obj: TurboFish{base: Ident("Vec"), ..}, name}` — `method_overloads` ключаются по declared type-name (`Vec`), а match покрывал только `Ident`/nested-`Member`-ресиверы. Без arm variadic-routing не fire'ил → call-site packing (сбор args в `[]T`) пропускался → `_static_of(1,2,3)` «too many arguments» (тело ждёт один собранный `NovaArray_nova_int*`). **Фикс:** добавлена `TurboFish{base: Ident(n)}`-arm в `recv_method` (lookup_variadic_arity) → ключ `(n, method)` в `method_overloads`; existing variadic-routing (synth `ArrayLit` + mono-static-dispatch) делает остальное zero-copy. `Vec[int].of(1,2,3)` / `Vec[int].of()` (empty) / `Vec[str].of("a","b")` — все PASS. Фикстура `plan153_0/variadic_of` (3 теста). 0 регрессий (plan90/90_1/99/101/131/138/153/basics/generics + pre-existing baselines неизменны). **Scope-note:** фикс покрывает `Vec[T].of` (turbofish-форма, канон под D239 `[]T≡Vec[T]`); `[]T.of` array-ext static-форма — ОТДЕЛЬНЫЙ broader gap (user-defined static-методы на `[]T`-ресивере не диспетчеризуются вообще, даже non-variadic — только `new`/`with_capacity` builtins), не входит в этот маркер. | Plan 153.0 | ✅ DONE |
| `[M-153-d239-explicit-vec-to-slice-param]` | Plan 153.0 (pre-existing на main). Residual D239 non-transparency: значение с ЯВНОЙ аннотацией `ro v Vec[int] = …` НЕ коэрсится в `[]int`-параметр (`E7301`), хотя ИНФЕРИРОВАННЫЙ `ro v = Vec[int].from(…)` коэрсится (t8/plan138_1 PASS). Type-checker coercion: явный `Vec[T]` → `[]T`-param должен быть прозрачен (alias). Pin: `plan153_0/neg/vec_explicit_annotation_to_slice_neg`. Закрытие = «убрать остаточные спец-кейсы []T» из 153.0 scope (отложено как compiler-coercion fix). | Plan 153.0 / type-checker | P2 |
| `[M-153-vec-compare-u8-memcmp-fastpath]` | Plan 153.0 (perf-only). `Vec[T Compare] @compare` теперь поэлементный (корректно для всех T). `Vec[u8]` мог бы взять memcmp fast-path (для u8 байтовый = поэлементный, вектори­зуется), но это требует u8-специализации (type-dispatch на element). Не корректностный — perf. | Plan 153 / perf | P3 |
| `[M-153-vec-combinators-prelude-global]` | Plan 153.0/153.2. Eager-комбинаторы вынесены в отдельный explicit-import `collections.vec_seq` (не prelude-global), т.к. их идентификаторы (`[Acc]`/`f`/`op`) засоряют каждый юнит (shadow `type Acc` / collision `fn f`/`fn op`) — корень `[M-codegen-var-types-fn-scope]` + D145. Плана 153.2 переделывает их в LAZY-адаптеры на VecIter; пересмотреть, может ли lazy-слой стать prelude-global (после фикса scope-leak, или дизайном без поллюции). | Plan 153.2 | P3 |
| `[M-153-scalar-min-max]` | ✅ **CLOSED 2026-06-16.** `@min(other)`/`@max(other)` на всех 12 числовых типах (int/u8/u16/u32/u64/uint/i8/i16/i32/i64/f32/f64) в `std/runtime/defaults.nv`. Тест `plan153_1/scalar_min_max` PASS. Коммит `782a8e36`. D239 §2. | Plan 153.1/153.2 Ф.0 | ✅ DONE |
| `[M-153.2-generic-over-source-zerocost]` | 🟡 PARTIAL (Stage 2 реализован; Stage 3 ЧАСТИЧНО — 5 blanket терминаторов через Plan 161 Ф.0-Ф.4; adapter chain-entry остаётся per-type). Plan 153.2 (perf-only, НЕ упрощение). Zero-cost generic-over-source слой `collections.vec_iter_zc` зашиплен: адаптеры — generic-over-source `value`-рекорды (`MapIter[I,T,U]`/`FilterIter[I,T]`), источник инлайн полем `src I`, `@next()` статически диспетчит `(@src).next()`; цепочка мономорфизируется в один вложенный тип. Измерено: per-adapter allocs 3→0, source-box 9→0, step-индирекция убрана. **Stage 1 (by-value mono generic value-records, commit `0da18125`):** `BoxIter[T]` помечен `value` → wrapper-record 5→0 heap-allocs (`grep nova_alloc(sizeof(Nova_BoxIter` = 0 в plan153_2/*.c). **Stage 2 (generic-over-source, commit `515de574`).** **Stage 4 (alloc-free терминаторы + `collect_into`, 2026-06-15):** добавлен `mut @zcollect_into(out mut Vec[T]) -> ()` на каждый адаптер (тело = `zcollect` минус `Vec.new()` header-alloc; APPEND-семантика, `out.clear()` для reuse → амортизированный 0 alloc). Замер из C: все 4 `zcollect_into` = 0 `nova_alloc`; стриминг-терминаторы `zfold`/`zsum`/`zcount`/`zfor_each`/`zany`/`zall`/`zfind` подтверждены 0 `nova_alloc` (скаляр/bool/Option-акк, без out-Vec). Verdict: `.fold(0,…)` result-alloc=0, `.collect_into(out)` terminator-alloc=0. Фикстуры `plan153_2_zc/{collect_into,streaming_terminators}` 2/2 PASS. **Stage 3 (capture-free closure alloc-elimination, 2026-06-15):** closure-env/box больше НЕ аллоцируются для замыканий без захвата (file-scope static singleton) — chain `.map(\|x\| x*3).filter(\|x\| x%2==0).collect()` closure-allocs 4→0, `.fold` 6→0; см. `[M-153.2-Z-closure-devirt]`. Spec: [D277](../../spec/decisions/02-types.md#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2) (amends D228+D260). Остаток: сам ВЫЗОВ callback'а `f`/`pred` ещё fn-ptr-индирекция (devirt → `[M-153.2-closure-as-mono-type]`) + `VecIter`-source-курсор (heap ref-type). **Plan 162 (2026-06-16):** `EnumerateIter[I,T]` value-record добавлен в `vec_iter_zc` (D284) — `[M-153.2-enumerate-zc]` ✅ CLOSED; `take`/`skip` уже в vec_iter_zc (TakeIter/SkipIter). Детали в 153.2 Followups. | Plan 153.2 / D277 | P3 |
| `[M-153.2-closure-as-mono-type]` | 🟡 **PARTIAL (Stage 3 alloc-elimination landed 2026-06-15).** Plan 153.2 (perf-only). В `vec_iter_zc` callback-поля `f`/`pred` остаются boxed-замыканиями (`void*`+`NOVA_CLOS_CALL`, fn-ptr индирекция на элемент). Rust-style инлайн мэппера (devirt самого вызова) требует closures-as-mono-types (env как конкретный type-param). Отдельный крупный лифт поверх Stage 2 ([D277](../../spec/decisions/02-types.md#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2)). **Stage 3 (commit ниже) убрал АЛЛОКАЦИЮ closure-env**, см. `[M-153.2-Z-closure-devirt]`. Остаётся только fn-ptr индирекция вызова на элемент. | Plan 153.2 | P3 |
| `[M-153.2-Z-closure-devirt]` | 🟡 **PARTIAL (Stage 3, 2026-06-15).** Plan 153.2-Z (perf-only). **СДЕЛАНО — capture-free closure alloc-elimination:** замыкание БЕЗ захвата (env = `{int _dummy}`) теперь ОДИН file-scope static singleton (`nova_lambda_N_clos_singleton` + `_env_singleton`), а не два `nova_alloc` на call-site (env-box + `NovaClos_xx`-box). Безопасно: stateless-замыкание идентично всюду, static-адрес immortal (можно escape/store/outlive scope без dangling; Boehm видит как root). `emit_lambda` (emit_c.rs ~31427) — capture-free fast-path возвращает `(void*)(&singleton)`, drop обоих nova_alloc. Захватывающие замыкания (free_vars≠∅) — heap-путь без изменений (immutable by-value snapshot + mut by-ref box нужны per-instance). **Замер (release nova, C-codegen):** `v.ziter().zmap(\|x\| x*3).zfilter(\|x\| x%2==0).zcollect()` — closure-allocs в driver-теле **4→0**; `.zfold(0,\|acc,x\| acc+x)` — **6→0**. Verified: 0 `nova_alloc(sizeof(nova_lambda_N_env))` / `nova_alloc(sizeof(NovaClos…))` в сгенерённом C; все capture-free замыкания корпуса → singleton, PASS (vec_iter_zc/plan153_2/plan153_2_zc/plan99/plan70/generics/contracts/plan91_fe1 чистые; 0 регрессий — все наблюдаемые fail'ы pre-existing на захватывающих замыканиях heap-путём ИЛИ без замыканий вовсе, verified против baseline `bc4e02f5`). **ОСТАТОК (Stage 3 full):** (1) сам ВЫЗОВ `(@f)(x)` всё ещё через `NOVA_CLOS_CALL_xx` fn-ptr-макрос — НЕ инлайнится; true devirt = closures-as-mono-types (`MapIter[I,T,U,F]`, store `f F` by-value inline, `@next` зовёт `nova_lambda_N_body(&f,x)` напрямую) → `[M-153.2-closure-as-mono-type]`. (2) Захватывающие замыкания (mut-capture особенно) всё ещё heap-env. (3) `VecIter`-source-курсор — отдельный heap-ref-type alloc на `.ziter()` (свойство `VecIter[T]`, не замыкание). | Plan 153.2-Z / D277 | P3 |
| `[M-153.2-Z-noalloc-terminator]` | ✅ **CLOSED (Stage 4, 2026-06-15, commit `bf95d93d`).** Plan 153.2-Z (perf-only). Alloc-free терминаторы на zero-cost-адаптерах. **СДЕЛАНО:** (а) добавлен `mut @zcollect_into(out mut Vec[T]) -> ()` на каждый адаптер (`MapIter`/`FilterIter`/`FilterMapIter`) в `std/collections/vec_iter_zc.nv` — тело = `zcollect`-drain МИНУС `Vec[U].new()` header-alloc; пушит в caller-buffer. **Семантика APPEND** (НЕ чистит `out`; caller делает `out.clear()` для свежего sink → амортизированный 0 alloc при reuse); возвращает `()`, заполненный буфер виден через caller-биндинг (`Vec[T]` heap-ref). (б) подтверждено из сгенерённого C: все четыре мономорфизованных `…method_zcollect_into` тела = **0 `nova_alloc`** (vs `zcollect` с `…_static_new()`); стриминг-терминаторы `zfold`/`zsum`/`zcount`/`zfor_each`/`zany`/`zall`/`zfind` мономорфизованные тела = **0 `nova_alloc`** каждый (скаляр/bool/Option-аккумулятор, без out-Vec). **Verdict:** `.fold(0,…)` result-alloc=0; `.collect_into(out)` terminator-alloc=0. Фикстуры `plan153_2_zc/{collect_into (7 кейсов), streaming_terminators (5)}` 2/2 PASS; plan153_2 4/4 без регрессий. Чисто stdlib-врайринг (`std/*.nv` с диска, без ребилда). **Остаток (вне scope ступени 4):** chain residual heap = env пользовательских замыканий (Stage 3 `[M-153.2-Z-closure-devirt]`/`[M-153.2-closure-as-mono-type]`) + `VecIter` source-курсор (heap-ref). История — simplifications.md. | Plan 153.2-Z / D277 | ✅ DONE |
| `[M-153.2-tuple-elem-adapter]` | ✅ **CLOSED 2026-06-16.** Tuple-PRESERVING адаптер после `enumerate` (`.filter(|p| p.0…)`/`.take`/`.skip`) — работает. Баг был устранён в Plan 162/164 (codegen tuple-mono fix); тесты не запускались из-за `E_IMPORT_GLOB` в enumerate_*.nv. Фикс: `import std.collections.vec_iter as vec_iter`. 11/11 plan162 PASS. | Plan 153.2 / Plan 162 | ✅ DONE |
| `[M-153.2-Z-closure-devirt]` | План 153.2-Z Ступень 3 (perf-only, НЕ упрощение). Devirtualization замыканий **без захвата** (`\|x\| x*3`) в generic-over-source адаптерах: хранить замыкание в поле адаптера **по значению** (zero-size для capture-free) + мономорфизировать `next()` на конкретное замыкание → тело инлайнится прямой арифметикой, БЕЗ `NovaClosBase`/env-аллокации. Замыкания С захватом — env by-value в поле. Цель — убрать последние per-adapter heap-аллокации. Детали в [153.2-Z](153.2-Z-zero-alloc-lazy.md). | План 153.2-Z | P3 |
| `[M-153.2-Z-noalloc-terminator]` | План 153.2-Z Ступень 4 (perf-only, НЕ упрощение). Нулевой терминатор: подтвердить alloc-free стриминг-терминаторов (`fold`/`sum`/`count`/`for_each`/`any`/`all`/`find`/`min`/`max`) в сгенерённом C + добавить `mut @collect_into(out)` (сбор в переиспользуемый `Vec`, амортизированный ноль). `collect()` сохраняет 1 аллокацию (сам результат). Детали в [153.2-Z](153.2-Z-zero-alloc-lazy.md). | План 153.2-Z | P3 |
| `[M-153.5-flatten-nested-receiver]` | ✅ **РАЗРЕШЁН 2026-06-14** (Plan 153.5, branch `plan-153.5-restructure`, commits `1c323d0e` parser+mono + `16753d23` flatten). Вложенные generic-ресиверы произвольной глубины + `@flatten` реализованы. **Root-cause (обе половины):** (1) ПАРСЕР отвергал carrier `Vec[Vec[T]]` («expected `]`, got identifier») и схлопывал `[][]T`→`"[]T"`; (2) МОНОРФИЗАТОР биндил receiver-typevar `T` в *непосредственный* элемент (`Vec[int]`), не во *внутренний* (`int`) → неверный return-тип + segfault (mono'd `out` = `Nova_Vec____Nova_Vec____nova_int_p`). **Фикс (рекурсивный, depth-agnostic):** AST `Receiver.receiver_ty: Option<TypeRef>` несёт полный структурированный тип (единственное место, где глубина переживает — `type_name` flatten'ит); парсер принимает вложенный `parse_type` в carrier-слоте + рекурсивный сбор free-typevars (carrier) и считает глубину `[]` + спуск до внутреннего `Named` (slice); монорфизатор переиспользует рекурсивный `infer_type_param_binding` (bind `T` = innermost element, любая глубина) на ВСЕХ путях receiver-typevar-bind + depth-aware sentinel-ключи `"[]"*N+"T"`; flat `[]T` (depth 1) остался byte-identical, override гейтнут `receiver_ty_is_nested`. Checker collect'ит вложенные typevar'ы для `E_UNUSED_PREFIX_TYPEVAR` (но НЕ сидит scope из `receiver_ty` — сохраняет `E_UNDECLARED_TYPEVAR_IN_RECEIVER`). `@flatten` (`Vec[Vec[T]] @flatten() -> Vec[T]`, production carrier-форма) в `std/collections/vec/restructure.nv`. Фикстуры plan153_5_nested 4/4 (depth2/depth3/slice_nested/control_flat) + plan153_5/flatten. D263 AMEND + D145 AMEND. **Ортогональный pre-existing остаток (вне scope):** slice-форма `fn[T] [][]T -> []T` с телом, СТРОЯЩИМ `Vec[T].new()`, упирается в erased-base-body лимит (ломает и flat `[]T` с `Vec[T].new()` на baseline). История — `simplifications.md`. | Plan 153.5 | ✅ done |
| `[M-138.2-varindex-method-turbofish-misparse]` (был `[M-codegen-erased-stub-method-on-varindex-deref]`) | ✅ **FIXED on branch `plan-cgfix-erased-stub` (commit `6f74c0ba`, 2026-06-13), pending merge.** Discovered Plan 153.0; root-cause оказался **PARSER (D38 turbofish), НЕ codegen/erased-stub** (мисдиагноз). `@buf[i].compare(o.buf[i])` мис-парсился как turbofish static-call `@buf::<i>.compare(...)` — одиночный lowercase `i` парсится как валидное имя типа, `[i]`→type-arg, `.compare(` совпал с `Type[T].method(...)` формой → индекс ресивера терялся (turbofish transparent в codegen → `@buf` = голый указатель) → str-fallback dispatch → CC-FAIL `passing 'T*' to 'nova_str'`. **Fix:** `try_parse_turbofish_args` (parser/mod.rs ~5766+6493) — гейтить `.IDENT(` turbofish-продолжение по `base_is_type_like` (base перед `[` = Ident/Path = имя типа); `@buf[i]` (base=Member) роллбэчится в Index. **Минимальный репро:** `fn Bag[T Compare] @cmp(o Bag[T]) -> int { for i in 0..@n { ro c = unsafe { @buf[i].compare(o.buf[i]) }; … } }`. Триггер: generic-T ∧ var-index `[i]` (литерал `[0]` парсится не-как-тип → ОК) ∧ method `.IDENT(` после `]`. После мёржа фикса инлайн-форма работает → typed-locals workaround в `Vec.@compare`/`@equal` (protocols.nv) можно упростить. Permanent fixture `plan138_2/t17_var_index_inline_method_pos`. Residual: `[T]?` (try) на value-base структурно похож, не покрыт (не exercised). | branch plan-cgfix-erased-stub (pending merge) | P2 |
| `[M-138.2-bulk-insert-overload]` | Ф.0a Open-Question resolution: bulk-insert живёт как `@splice(i, Vec[T])`, НЕ как второй overload `@insert(i, Vec[T])`. Generic-method overloads коллапсят в монорфизации — `mono_method_decls` (emit_c.rs ~8404) keyed `(type, name)` с одним FnDecl на key, mono-sentinel `MethodSig` несёт пустой `param_c_types` (~8408) → `resolve_overload` не дизамбигуирует single `insert(i,T)` от bulk `insert(i,Vec[T])` для concrete `Vec[int]` (verified: оба роутятся на single, Vec-arg force-fit'ится в `nova_int v` → garbage). Plan 138.2 Ф.0a явно санкционирует `@splice`-rename как fallback. Fold обратно в `@insert` overload ждёт `[M-138.2-generic-method-overload-mono]`. | plan-138.2 Ф.0a Followups | P2 |
| `[M-138.2-generic-method-overload-mono]` | Codegen: generic-метод overloads должны переживать монорфизацию с per-arg-type routing. Сегодня `mono_method_decls` (emit_c.rs ~8404) = `HashMap<(String,String), FnDecl>` (один decl на (type, method-name), overload-коллапс) + mono-sentinel `MethodSig` с пустым `param_c_types` (~8408). Нужно: keyed-by-mangled-sig storage + concrete `param_c_types` в sentinel, чтобы `resolve_overload` (emit_c.rs:9913) дизамбигуировал по C-типам аргументов. Разблокирует fold `@splice`→`@insert` overload ([M-138.2-bulk-insert-overload]). | new codegen-план | P2 |
| `[M-138.2-nested-vec-elem-readback]` | DISCOVERED (pre-existing на plan-138.1, orthogonal к Ф.0a; вне зоны bulk-insert): `Vec[Vec[T]]` второй push+get читает повреждённый nested-элемент. Narrowed: `single push get0` PASS, `two push get0` PASS, `two push len2` PASS, но `two push get1` FAIL (читает не тот контент); `new+push`-вариант → CC-FAIL `unknown type name NovaOpt_nova_int_p` (отсутствует mono'd Option для nested-Vec-elem). Storage/codegen defect для `Vec[Vec[T]]`-элементов. Home: Plan 138.x nested-Vec follow-up. Низкий приоритет (single-уровень Vec корректен). | plan-138.x Followups | P2 |
| `[M-138.2-shadow-warn-post-flip]` | Ф.0c verified-deferral (для Ф.0-final): W_PRELUDE_SHADOW lint (`lint_prelude_shadow`, lints.rs:1459) fires ТОЛЬКО на prelude-visibility. Pre-flip `Vec` НЕ в prelude → explicit `import vec_owned.{Vec}` + user `type Vec` = ordinary import collision → warning корректно НЕ срабатывает (t14 = positive clean-compile, НЕ EXPECT_COMPILE_WARNING). После Ф.0-final (Vec в prelude) shadow юзерского `type Vec` ДОЛЖЕН surface W_PRELUDE_SHADOW; t16 (`#allow(shadow)`) пиннит suppress на тот момент. Checklist-item: после флипа добавить EXPECT_COMPILE_WARNING вариант t14 (Vec уже prelude-visible) и подтвердить W_PRELUDE_SHADOW + suppress. **NOT a defect — семантически правильное pre-flip поведение.** | plan-138.2 Ф.0-final | P2 |
| `[M-138.2-flip-erased-base-body-mono]` | ✅ **CLOSED (re-attempt #2, 2026-06-11).** Принципиальный фикс (не heuristic): Array-арм `type_ref_to_c` (emit_c.rs:5109) — generic-stub element (`is_generic_stub_c` && !contains `____`) → erase в `nova_int`. Тот же int64-erasure что legacy NovaArray `_`-арм (5188) + Named `any_erased` carve-out (5054). Concrete per-element Vec mono эмитится на каждом mono'd call-site. | plan-138.2 Ф.0-final | ✅ DONE |
| `[M-138.2-flip-value-pos-arraylit-vec-gate]` | ✅ **CLOSED по факту (re-attempt #2, 2026-06-11).** 4 contains_key("Vec")-гейта (Plan 138.1) уже Vec-gate-aware на value-position (array-literal `emit_array_lit`/`infer_expr_c_type`); универсальная prelude-доступность Vec-template — это и есть фикс рассинхрона (gate всегда ON для prelude-юнитов, graceful #no_prelude degrade). Проверено t3/t17 (Vec-free → typed Vec storage). | plan-138.2 Ф.0-final | ✅ DONE |
| `[M-138.2-flip-array-ext-vec-recv-routing]` | ✅ **CLOSED (re-attempt #2, 2026-06-11).** Принципиальный фикс (Plan 101.1 alt-key precedent): (a) call-routing (emit_c.rs:~22014) splice'ит `("[]T",m)` sentinel в candidates когда direct-key пуст для `Vec____*` receiver; (b) worklist-drain (emit_c.rs:~3060) роутит `Vec____*::m` worklist-key на `[]T` base FnDecl (recv_type=`Vec____<elem>` → typed-Vec-receiver instance); (c) elem-extract de-mangle через `generic_type_instance_info` (string-strip `Vec____` даёт mangled `Nova_Wrap_p`, не C-тип). Mirror в `infer_expr_c_type` (~30968). plan91_fe1 8/2→10/0. | plan-138.2 Ф.0-final | ✅ DONE |
| `[M-138.2-parfor-vec]` | **OPEN (Ф.2.1, sanctioned exception, 2026-06-11).** parfor (D71) использует `NovaArray_{nova_int,nova_bool,nova_f64,nova_str}` для internal result-collection буфера (emit_c.rs:7242/7290) — internal codegen-путь, не user-facing `[]T`: layout-identical с Vec, никогда не escape'ит, ограничен 4 примитивами. Миграция на Vec требовала бы Vec-template в каждом concurrency-юните (graceful-degrade риск) ради нулевого семантического выигрыша. RETAINED как documented exception. Re-attempt = когда (если) NovaArray retire завершится после Plan 139 Ф.2. parallel_for 2/0, parallel_for_array 1/0 (GREEN). | plan-138.2 Ф.2 | P3 |
| `[M-138.2-closure-array-vec]` | **OPEN (Ф.2.2, sanctioned exception, 2026-06-11; = ранее `[M-138.1-closure-array]`).** `[]fn(...)` → `NovaArray_void_p*` (emit_c.rs:5106-5107, explicit exclusion из `[]T`→Vec flip). Closures = `void*` (`NovaClos_X*`); `Vec[fn]` mono требует closure-as-element schema, которой нет. Feasibility-investigation: closure-array fixtures (plan55 f1_closure_array_with_capture/f1_fn_array_collect_positive/f1_negative_fn_array_arity_mismatch) все PASS на `NovaArray_void_p`-пути. RETAINED как documented exception. Re-attempt вместе с финальным NovaArray retire (Plan 139 Ф.2). | plan-138.2 Ф.2 | P3 |
| `[M-138.5-right-binding-migration]` | ✅ **CLOSED Plan 147 Ф.2-3 (2026-06-12).** prefix→postfix `*T` модель landed полностью: parser `E_POINTER_PREFIX_MODIFIER` (prefix `mut */ro *`), codegen pointee preservation (138.4 G-D), И УБРАН flip-scan codegen-SEED (`field_type_with_binding_mut`/`promote_pointer_pointee_mut`, авто-промоутил `mut data *T`→`Pointer(Mut(T))` от binding) — 3-axis/D246 запрещает наследование pointee-mut от binding; mut-pointee = явный `*mut T`. Постфикс-канон = единственная модель (flip-scan RETRACTED). | plan-147 (closed) | ✅ DONE |
| `[M-138-range-value]` | Range — reference-record, не value-record; Plan 138 Ф.0.3 migration не сделана. (138.5 трогает range.nv — re-confirm.) | plan-138 Followups | P2 |
| `[M-138-unsafe-block-postfix-stmt]` | ✅ **CLOSED Plan 148 Ф.2 (2026-06-12).** Расследование: парсер УЖЕ корректен — у `parse_stmt_or_expr` нет отдельной leading-`unsafe`/block-statement ветки; `_`-arm зовёт полную `parse_expr`→`parse_postfix`, поэтому block-формы (`unsafe {}`/`if`/`match`/bare `{}`) в statement-позиции принимают постфикс (`.method()`/`[i]`/`.field`) напрямую, без `(…)`. Проверено value-discarded формой (true `Stmt::Expr`). Фактическая работа: (a) cleanup — убраны лишние скобки vec_owned.nv:862/874 (`(unsafe { @data[i] }).display/debug` → без скобок), build/run зелёные (vec_debug_pos pre-existing fail тот же с/без скобок); (b) regression-guard фикстуры `nova_tests/plan148/up_unsafe_postfix_stmt_ok` (8/0: unsafe `.method`/`.field`/`[i]`, `if`/`match` постфикс, value-discarded stmt) + `up_bare_block_stmt_ok` (3/0: A2 — bare `{}` остаётся statement); (c) D49 (03-syntax.md) amend — задокументирована единая postfix-on-block-expr-in-stmt семантика + граница с bare `{}`. plan148 9/0. | plan-138 (closed) | ✅ DONE |
| `[M-138-double-pointer-codegen-test]` | `**T`/`***T` **парсятся** ✅ (Star-arm рекурсивен, parser/mod.rs:5214; нет `**` power-токена), per-level postfix pointee-mod консистентен (`*mut *ro T`). НЕ проверено: codegen `Pointer(Pointer(T))` → C `T**` + модификатор-комбо end-to-end. Нужен тест + D216 doc-note «N-level pointers, per-level postfix pointee-mod». Use: FFI `char**`/argv, out-params. | plan-138 Followups | P2 |
| `[M-138-binding-type-mut-conflict]` | ✅ **CLOSED Plan 147 Ф.6 (2026-06-12, D246 P6 split).** Visibility-aware диагностика НЕ нужна: 3-axis модель (L1 binding × L2 view, ортогональны) прямо разрешает обе пары `ro X mut T` (reassign❌/content✅) и `mut X ro T` (reassign✅/content❌) как ЯВНЫЙ opt-in R2-split — это не «конфликт», а две независимые оси. `ro X mut T` валиден всегда (mut = L2 content-view), enforce'ится через `root_view_is_mut_type`/`root_view_is_ro_type` в check_target_readonly. Oracle A/B покрывают (a3/a4/b1/b4). | plan-147 (closed) | ✅ DONE |
| `[M-ptr-cast-reinterpret-unsafe]` | **OPEN (учтён в coercion Plan 147 Ф.3/D246).** Ro-laundering ветка `(a) *ro T → *mut T widening` **ПОКРЫТА** D246: `*T→*mut T` (= `*ro→*mut` под `*T≡*ro T`) запрещён в coercion, `*mut→*T` авто-сужение. ОСТАЁТСЯ ветка `(b) *T → *U` (смена pointee-типа, `*u8→*int` = OOB/align/aliasing UB) — должна требовать `unsafe`/`E_PTR_CAST_REINTERPRET`; это ОТДЕЛЬНАЯ ось от ro-mut, D246 её не закрывает. Сейчас `as *U` = safe-reinterpret. D216 cast-rules amend. | plan-138.5 Followups | P2 |
| `[M-138-canonical-modifier-order]` | ✅ **CLOSED Plan 148 Ф.1 (2026-06-12, D241 enforcement).** Parser (`parse_type_decl` modifier-loop) присваивает каждому модификатору canonical rank (`value`=0 → `consume`=1 → `priv`=2), проверяет монотонность и при инверсии эмитит `E_MODIFIER_ORDER` с machine-applicable fix-it (переписывает регион в rank-канон). Обобщено на ВСЕ type-модификаторы (новый модификатор = rank по scope). `plan124_8/modifier_order_independence_ok` ФЛИПНУТ в negative. pos+neg фикстуры `nova_tests/plan148/mo_*` (7/0). D241 (03-syntax.md) IMPLEMENTED + amend D124/D220 (02-types.md). | plan-138 (closed) | ✅ DONE |
| `[M-codegen-unify-tuple-repr]` | ✅ **CLOSED Plan 148 Ф.4 (2026-06-12, D123 amend).** Кортежи унифицированы на on-demand mono'd typed структуры. (A1) Concrete tuples (f64/str/record/bool) хранят реальные C-типы полей — без int-boxing/`(intptr_t)`-каста (уже был default через mono'd путь; подтверждено фикстурой). (A2) Blanket `_NovaTuple1..8` pre-decl РЕТАЙРНУТ — legacy all-int `_NovaTupleN` эмитится on-demand per requested arity (`register_legacy_tuple` + `/*__LEGACY_TUPLE_TYPEDEFS__*/` splice, idempotent `#ifndef`); на практике только arity 2 (erased `HashMap`/`Set` `(K,V)`). (A3) tuple eq/debug корректны по типам (per-element compare через `emit_field_eq`). КЛЮЧЕВОЙ ФИКС: field-read (`emit_expr` Member-digit) + `infer_expr_c_type` декодируют элемент-тип напрямую из mono'd struct name в `obj_ty` (`parse_mono_tuple_elements`), не только из per-Ident side-table — чинит field access на fn-параметрах, call-result кортежах и **вложенных** `t.0.1` цепочках. Закрыло 5 pre-existing CC-FAIL в plan59 (f2/f10/f13/f15/f16). Новый код `[E_TUPLE_DESTRUCTURE_ARITY]` на 3 codegen-сайтах. (A4) полная регрессия зелёная (~30 директорий vs baseline-бинарь в temp-worktree: 0 новых FAIL, +5 fixed). Фикстуры `nova_tests/plan148/tr_*` (2 pos: `tr_tuple_typed_fields_ok`, `tr_tuple_eq_debug_ok`; 2 neg: `tr_tuple_destructure_arity_neg`, `tr_tuple_destructure_overlong_neg`). plan148 17/0. Остаток (НЕ блокирует): mono'd-tuple-of-Vec forward-decl ordering (plan59 f5 / types arrays — pre-existing, отдельная ось); out-of-range tuple field index не reject'ится (pre-existing checker gap). | plan-148 (closed) | ✅ DONE |
| `[M-hashmap-tuple-key-mono]` | **OPEN (обнаружено Plan 152.4, 2026-06-14).** `HashMap[(int,int), int]` (кортеж как ключ): `nova check` **ПРИНИМАЕТ** (тип-уровень OK), но codegen даёт **CC-FAIL** (НЕ silent — hard error). Полу-инстанцированный mono: переменная объявляется как `Nova_HashMap____nova_int__nova_int*` (ключ-кортеж ошибочно стёрт в `nova_int`), конструктор использует полный mangle `Nova_HashMap_____NovaTuple_2_8_nova_int_8_nova_int__nova_int_static_new()`, а `insert` уходит в **erased** `Nova_HashMap_method_insert(m, (void*)(intptr_t)(tuple), …)` — три рассогласованных пути; typedef tuple-key варианта не эмитится → `use of undeclared identifier 'Nova_HashMap_____NovaTuple…'`. Корень: mono-имя HashMap считает только примитивные/Named-ключи, кортеж-ключ не получает `Hash`/`Equal` авто-derive (Plan 126 покрыл records, не кортежи) и mono-typedef не эмитится. Решение (развилка): (a) codegen — поддержать tuple-key (авто-derive `Hash`/`Equal` для кортежей + полный mono-typedef + единый key-mangle на всех путях), ЛИБО (b) checker — отклонять `K`-без-`Hash` на тип-уровне с `E_HASHMAP_KEY_NOT_HASH` (тогда честный отказ вместо CC-FAIL). Связь Plan 154 (checker over-accepts → codegen не доставляет; но это CC-FAIL, не silent-noop). Репро: `HashMap[(int,int),int].new(); m.insert((1,2),42); m.get((1,2))`. Workaround: упакованный int-ключ `(a<<21)\|b` в `HashMap[int,int]` (использован в 152.4 для composition-таблицы). | floating (codegen / checker) | P2 |

## P2 — Perf optimization (escape/Z3-driven; correctness-neutral)

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-opt-auto-scoped-ref]` | Escape-analysis авто pass-value-param-by-ref + return-slot elision (NRVO); обобщить ресивер-`&obj`. | new perf-план (value-types thread) | P2 |
| `[M-opt-value-sum-types]` | Compiler-inferred value(stack)/heap для sum-типов (recursion+size+escape; прозрачно — immutable); payload-less интернирование. | new perf-план (Plan 120/139) | P2 |
| `[M-opt-elide-proven-overflow-checks]` ✅ **CLOSED 2026-06-14** ([Plan 140.4](140.4-overflow-check-elision.md), branch `plan-140-overflow-elide`, D272) | Z3/range-элизия доказуемо-безопасных `int`-overflow чеков (proven→elide, реализует отложенную «Plan 33.8 Ф.1.3»). `int` = безграничный Z3-Int → пруф = range-fit `INT64_MIN <= a OP b <= INT64_MAX` под loop/литерал/`requires`-фактами; sound by construction (операнды не ограничиваются искусственно); `*` нелинеен → Unknown → консервативно чек. Зеркало Plan 140.2: verify `prove_int_overflow_sites` (двухпроходный, non-trivial gate) → 2 proven-множества (always-safe элидится всегда даже под `--contracts=off`/`#unchecked`; contract-based — лишь при enforced `requires`); codegen `overflow_site_elided(span)` гейтит `emit_expr`+`emit_expr_with_target_type`. **Scope V1:** binary `+`/`-`/`*`; compound-assign отложен → `[M-140.4-compound-assign-overflow-elision]`. C-инспекция: always-safe `i+j` z3-off=0/Trivial=1, contract-based `a+b` enforce=0/off=1/Trivial=1, mul bounded=0/unbounded=1. Тесты plan140_4 5/0 (OE1-OE5) + регрессии contracts 314/0, plan140 51/0, basics 8/0, Trivial 1/0/4skip (OE6/OE7). D272 + D24/04-effects amend. | Plan 140.4 | ✅ DONE |
| `[M-140.4-compound-assign-overflow-elision]` | Plan 140.4 V1 покрывает binary-выражения `int` `+`/`-`/`*`; compound-assign (`x += y`, codegen site `emit_c.rs` AssignOp→`nova_int_checked_*`) отложен — таргеты обычно безграничные аккумуляторы (`sum += a[i]`, рідко доказуемо), отдельный `Stmt::Assign` AST-путь со своей span-привязкой → высокая стоимость, near-zero реализуемая элизия. Чек остаётся (sound). Доделать, если появится мотивирующий кейс (compound-assign с loop/requires-bounded таргетом). | Plan 140.4 followup | P3 |
| `[M-opt-preempt-strided-loop]` | `nova_preempt_check()` в back-edge КАЖДОГО цикла блокирует clang-векторизацию. **✅ Part A+B DONE (Plan 143, merge `7c047a1b`, 2026-06-14):** (A) skip preempt-check на provably-short const-bound range-циклах (оба bound'а — int-литералы, count ∈ [0,1024] → starvation-safe, разблокирует векторизацию); (B) `for i in lo..hi { dst[i]=src[i] }` на Vec[T] → overlap-safe bulk copy (memmove fast-path + element-loop fallback на destructive forward-overlap; inclusive-overflow-safe). **Adversarial-review (6 агентов) + эмпирический probe** нашли 2 бага — закрыты: writable offset-overlap view'ы (`a=v[1..]; a[i]=b[i]`) пропагируют ≠ memmove → runtime overlap-guard; inclusive `hi+1` при i64::MAX = UB → `last` без +1. plan143: 9 copy + 3 preempt кейса PASS; регрессия 0 new fail (Vec/collection слайс + concurrency). **🟡 Остаётся long-term:** signal-based async preemption (Go 1.14 SIGURG) — general для variable-bound циклов без per-iteration call. **Cross-link Plan 144:** SIGURG = ОБЩИЙ async-yield (preempt + GC safe-point, [§7.4](144-precise-gc-implementation.md)). | Plan 143 (A+B done) / SIGURG → 144 §7.4 | 🟡 A+B done, SIGURG P2 |
| `[M-opt-leaf-preempt-entry-elision]` | ✅ **DONE (Plan 143.2, D271, ветка `plan-143-leaf-preempt-elision`, 2026-06-14):** function-prologue `nova_preempt_check()` (Plan 44.7) элидируется на provably-leaf функциях через source-level whole-program pre-pass (`preempt_keep.rs`): call-граф над FnDecl + per-fn флаги indirect/ffi/address_taken + Tarjan SCC, `KEEP = cycle ∪ indirect ∪ ffi ∪ address_taken`, ELIDE иначе. Соундно над монтоморфизацией (KEEP-статус шаблона наследуется всеми моно/erased-инстансами тем же ключом). Conservative default = KEEP при любом сомнении. plan143_2 7/7 (positives элидированы, negatives KEPT — verified в C); регрессия 0 net-new fail. Adversarial-review закрыл реальную дыру (рекурсивный generic терял safepoint в моно-инстансе). Commits a35e4df7+d37dd913+c2d98141+9de1cbcf. **Остаток (Q-loop-opt-thresholds §B, minor):** cross-module precision, minimal-SCC-cut, synthesized-conversion arg-typing; SIGURG (Plan 144 §7.4) general для variable-bound. | Plan 143 §2.B | ✅ done |
| `[M-144.0-may-gc-effect-analysis]` | ✅ **DONE — анализ-слайс (Plan 144.0, D273, ветка `plan-144-may-gc-effect-analysis`, 2026-06-14):** may-GC эффект-решётка (дефолт MayGC top, NoGC доказывается) closes Plan 144 soundness-дыру **H4** ([§7.6](144-precise-gc-implementation.md#76)). Source-level whole-program pre-pass `may_gc.rs` (переиспользует call-graph/`fn_key`/overload/Tarjan из `preempt_keep.rs`): seed self-MayGC = аллокация ∪ indirect ∪ FFI ∪ unresolved-callee ∪ first-class-method-value; allowlist provably-non-allocating (unknown → allocates); SCC-конденсация + обратная пропагация MayGC по коллерам. Соундно над монтоморфизацией. **Emit-nothing** (CLI `nova gc-effect-analyze` json/text + unit-тесты ONLY; `emit_c.rs` не зовёт). plan144_0 10 фикстур (3 NoGC + 7 MayGC) + 19/19 unit PASS; release чистая, C не изменён; adversarial-review закрыл method-value ложный NoGC (commit `b908acb2`). Commits `6276d729`+`60590d21`+`13042ed8`+`b95f3854`+`b908acb2`. **🟡 Остаётся:** O1-потребление набора (frame-elision / write-back-skip) = **Plan 144 Ф.2** (отдельно, под гейтом); residual-точность = [Q-may-gc-precision]. | Plan 144.0 (анализ done) / O1 → 144 Ф.2 | ✅ analysis done, O1 P1 |
| `[M-144.1-heap-bitmaps]` | ✅ **DONE — аналитическая (emit-nothing) половина (Plan 144.1, D277, ветка `plan-144.1-heap-bitmaps`, 2026-06-15):** per-type GC pointer-offset bitmap для каждого типа (record / sum **per-variant** / nested-value рекурсия / scalar-only / heap-контейнерные поля) — heap-сторона Plan 144 §7. `gc_layout.rs` (`compute_gc_layout` / `GcLayoutMap` / `LayoutInfo` / `VariantLayout` / `classify_field`). **Layout-точно** — boxing-aware per-variant/field walk (согласован с `emit_c::type_decl_size_or_align`), три math↔emit-расхождения (`[N]T`/`[]T` FIELD → один heap-ptr; `char`→`nova_char` 8б) примирены сайзингом по эмитимому C. `str.ptr`@0 = указатель (object-start lookup на mark, §7.6 H1), `len` скаляр. Sum **per-variant** (не union — union сканировал бы скаляр неактивного варианта). **Conservative default** (unknown/erased/opaque/protocol → указатель; `unresolved=true` → весь объект консервативно; никогда не пропустить → no UAF). raw `*ro u8` = НЕ-GC скаляр (вне heap). **Emit-nothing** (CLI `nova gc-layout-analyze` text/json + unit-тесты ONLY; `emit_c.rs` не зовёт — проверено grep'ом). plan144_1 **6 фикстур** (incl. recursive) + **22/22 unit** PASS; release чистая, C байт-в-байт не изменён. Commits `ca42ea9f`+`40c365aa`+`89d12c2d`+**`15b6384c`** + docs. **⚠ Независимый аудит поймал ревью-пробел** (F3-фикстуры НЕ включали рекурсивных типов): `gc-layout-analyze` падал stack-overflow'ом на ВАЛИДНОМ рекурсивном sum (`Tree=Leaf|Node(Tree,Tree)`, который `nova check` принимает) — причина: мёртвый size cross-check `layout_of_sum` звал boxing-неосознающий `const_fn_eval::type_size_or_align_resolved` (инлайнил поле собственного типа sum вместо боксированного указателя → ∞-рекурсия). Убран (per-variant walk авторитетен); +регресс unit-тест `recursive_sum_does_not_overflow` +фикстура `recursive.nv`. **🟡 Остаётся:** runtime-потребление (layout-id в заголовке объекта + точный mark-sweep tracer) = **[Plan 144.5](144.5-nonmoving-precise-gc-online.md) Ф.5**; residual-точность (closures env-bitmap / generic-erased / FFI-edge) = [Q-gc-layout-precision]. | Plan 144.1 (аналитика done) / runtime → 144.5 Ф.5 | ✅ analysis done, runtime P3 |
| `[M-parser-record-field-separator]` | ✅ **CLOSED 2026-06-15** (commit `64ac38fa`, merge `9b47f954`). **Парсер-баг:** оба branch'а `if/else` в `parse_record_fields_with_default` делали одинаковый `skip_newlines()` → `{ x int y int }` молча парсил 2 поля без разделителя. По D49+D215: на одной строке запятая **обязательна** (как и в named-tuple). Фикс: `else`-branch теперь требует наличия `Newline`/`Semicolon`/`RBrace`, иначе **`E_RECORD_FIELD_MISSING_SEPARATOR`** с подсказкой. Heap и value record оба покрыты. **Acceptance criteria:** neg `{ x int y int }` → ошибка со span на `y`; pos `{ x int, y int }`, `{ x int\n  y int }`, trailing-comma — PASS. 4 фикстуры `nova_tests/plan_parser_recsep/` (2 neg + 2 pos, через релизный nova check/test); 2 inline test-строки в `escape_analyze.rs`+`field_cache.rs` мигрированы (comma-added); 827/0 lib-тестов; std/ 0 ложных E_RECORD_FIELD_MISSING_SEPARATOR. Спек: [D215 amend](../../spec/decisions/02-types.md#d215-amend). | floating (parser correctness) | ✅ DONE |
| `[M-checker-recursive-type-overflow]` | ✅ **CLOSED 2026-06-15** (branch `fix-checker-recursive-type-overflow`, commits `219be59a` fix + `1245ef68` tests). **Boxing-aware type-size + recursion-guard** — больше НЕ stack-overflow на рекурсивных типах. Корень: два unguarded boxing-неосознающих рекурсивных walk'а. (1) `const_fn_eval::type_size_or_align_resolved` (бэкает `size_of`/`align_of` + все introspection-инструменты) инлайнил object-size heap-поля и ∞-рекурсировал; теперь **boxing-aware** — ссылка на Named heap-record / любой sum short-circuit'ит на pointer-size (8/8), что **совпадает с эмиссией emit_c** (`Nova_X*`; зеркалит `gc_layout::classify_named_decl`); только value-record/named-tuple/newtype/alias спускаются в inline-layout. Это делает ВАЛИДНЫЙ рекурсивный heap-тип (`type H { t Tree }`, `type Tree \| Leaf \| Node(int,Tree,Tree)`) конечным (рекурсивное поле — указательный лист). Public-сигнатура сохранена через внутренний depth-threaded хелпер (budget 128), который на runaway value-self-cycle (`type N value { next N }`) возвращает `None` вместо overflow. (2) `types::type_is_consume` следовал named-ссылкам без visited-set → ∞-цикл на рекурсивном типе в `check_linearity_markers`; добавлен visited-set guard. **`type H { t Tree }` теперь check/build'ится; value self-ref больше НЕ крашит.** **size_of-семантика (clarification):** `size_of[heapT]` теперь = 8 (pointer) — это КОРРЕКТНОЕ reference-semantics-значение (переменная heap-типа = `Nova_X*`), не inline object-size; emit-accurate. Blast radius: 2 gc_layout cross-check'а + 2 фикстуры обновлены с заметкой (без production-ослабления); plan114_4_4 40/2 (2 FAIL pre-existing HOF-trampoline, идентичны baseline); codegen lib 22/0 gc_layout. Фикстуры `plan144_checker/{heap_embed_pos,value_self_ref_neg}` 2/2 PASS через C-codegen. Спек: [D280](../../spec/decisions/08-runtime.md#d280). **Q-остаток ЗАКРЫТ (2026-06-15):** `E_INFINITE_TYPE` теперь реализован — value-containment cycle-detector в `check_type_decl` (`types/mod.rs`, коммиты `ae1e2906`+`3ffa7714`, ветка `fix-infinite-value-type`); 5 neg + 6 pos фикстур (`nova_tests/plan144_inftype/`); `nova check std/` → 0 false-positive; [Q-infinite-value-type](../../spec/open-questions.md#q-infinite-value-type) ✅ RESOLVED. | floating (checker robustness) | ✅ DONE (incl. Q) |

## P2 — Ergonomics / stdlib combinators

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-161-blanket-receiver]` | ✅ **Ф.0-Ф.4 CLOSED (branch plan-161, 2026-06-15).** V1: 5 blanket terminators (`@zfold`/`@zcount`/`@zfor_each`/`@zany`/`@zall`) на `Next[T]` в `vec_iter_zc.nv`. G-F codegen (typevar-ресивер dispatch + bound-consult) landed. 10/10 plan161 PASS. V2: `[M-161-parametric-return]`. | Plan 161 | P2 |
| `[M-161-parametric-return]` | ✅ **CLOSED 2026-06-16 (Plan 161 V2, commits `776447ab`+`9065c637`+`3ba2dacc`).** Blanket методы с параметрическим return type T/Option[T]/Vec[T] работают: T-subst в return type при mono реализован в emit_c.rs. 12/12 plan161 PASS. | Plan 161 V2 | ✅ DONE |
| `[M-combinators-completion]` | Добавить `find` (short-circuit→`Option[T]`), `flat_map` (nested comprehensions), `zip` (parallel iter); обобщить `sum`/`min`/`max` с `[]int`-only → generic `[]T` (Num/Comparable bound). НЕ нужны: `collect` (комбинаторы eager), `take`/`skip` (в vec_iter_zc TakeIter/SkipIter ✅), `reduce` (fold), `count` (filter+len). **`enumerate` ✅ CLOSED** (Plan 162, 2026-06-16 — `EnumerateIter[I,T]` zero-cost в `vec_iter_zc`, D284). | new stdlib-combinators mini-план | P2 |
| `[M-opt-iter-generic-combinators]` | Комбинаторы (map/filter/fold/any/all/…) generic над `Iter[I]`, не только `[]T`-ресивер → работают на Range/HashMap/custom без материализации в `[]T`. Главный рычаг comprehension-эргономики (Python-comprehension работает над любым iterable). | new stdlib-combinators mini-план | P2 |

## P2 — Const-fn / Language features

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-114.4.4-configurable-iterations]` | Const-fn eval loop-limit hardcoded 10_000 (6 sites), нет override. | plan-114 Followups | P2 |
| `[M-114.4.4-let-destructure]` | `let (a,b)=`/record destructure в const-fn body не поддержан (`E_CONST_FN_PATTERN_NOT_SUPPORTED`). | plan-114 Followups | P2 |
| `[M-115-newtype-multiarg-constructor]` | Multi-arg newtype `type X(A,B)` не поддержан (single-arg-only в emit_c). | plan-115 Followups | P2 |
| `[M-generic-param-bound-with-constraint]` | **Feature:** `fn[I Next[T Hash]]` — bound на параметре протокола (`T Hash` внутри `Next[T]`). Сейчас `T Hash` в позиции generic-аргумента (`TypeRef`) не является синтаксически валидным — аргументы протокола это `Vec<TypeRef>`, не `Vec<GenericParam>`. Workaround: два параметра `fn[I Next[T], T Hash]` с workaround-порядком (см. `[M-codegen-blanket-generic-param-order]`). Желаемый синтаксис позволил бы выражать constraint в одном параметре и не зависеть от порядка. | floating / language | P2 |
| `[M-impl-attr-generic-protocol]` | ✅ **CLOSED (Plan 164 Ф.1, 2026-06-16, commit `3846a976`).** `#impl(Next[U])` теперь парсится корректно: bracket-skipping loop в `parse_type_attrs()` `"impl"` arm хранит полный spec (`"Next[U]"`, `"Next[(int,T)]"` и т.д.); `impl_spec_base_name` для дедупликации. `types/mod.rs`: `verify_impl_protocols` использует `impl_spec_args_text` для proto_arg_subst + `check_signature_match_with_subst` + `normalize_type_str`. `vec_iter_zc.nv`: все 6 adapter `@next()` аннотированы `#impl(Next[T/U/(int,T)])`. `VecIter.@next()` тоже. Тесты: `plan164/impl_attr_generic_pos` + `_neg` 2/2 PASS. | Plan 164 Ф.1 | ✅ done |

## P2 — Concurrency / Backend / Tooling / Stdlib

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-116-openssl-backend]` | Опц. OpenSSL TLS 1.0/1.1 handler (rustls = default); Plan 116 не начат (PLANNED). | plan-116 Followups | P2 |
| `[M-173-detach-escalate-to-scope]` | `detach` error-policy opt-in `escalate-to-scope` (D414 §2): привязать сироту к enclosing `supervised`-scope → ошибка участвует в §1-precedence вместо LogAndDrop. Нужен scope-handle у detach-примитива + участие в decision-loop. Дефолт LogAndDrop не менять (owner-gated). Checker-enforcement `Detach` + LogAndDrop-дефолт уже сделаны (Ф.3 п.2). | Plan 173 Ф.3 | P3 |
| `[M-91.fe5-math-time-conformance]` | math (sqrt/ln) есть; Instant/Duration time-API conformance pending. | plan-91 Followups | P2 |
| `[M-ide-integration-deferred]` | ✅ **CLOSED 2026-06-17 (Plan 104 ВСЕ sub-plans 104.0–104.9).** Production LSP полностью построен: completion/refs/rename/format/code-actions/symbols/tree-sitter/editor-packaging/close-out. 268 tests PASS. | plan-104 Followups | ✅ done |
| `[M-104.2-cross-file-goto]` | ✅ RESOLVED (Plan 104.10 Ф.3, 2026-07-03). `goto_definition.rs` → cross-file через provenance: `span.file_id` → `file_map` (из реальных `peer_files`, НЕ grep) → `Location` в файле-цели; range disk-authoritative для peer'ов, in-memory для entry. Server использует Ф.1-кеш. 16 unit + `e2e_smoke::pos11` (JSON-RPC cross-file → prelude). | plan-104.10 Ф.3 | ✅ done |
| `[M-104.10-vfs-overlay-imports]` | 🟡 PARTIAL (Ф.3). Range peer-цели считается по диску (span'ы disk-relative из `resolve_imports_inline`/`parse_with_file_id`). Единый VFS-overlay открытых буферов поверх import-резолва И позиционирования (несохранённая правка peer'а сдвигает goto live, zls-стиль) отложен — требует чтения открытых буферов в `resolve_imports_inline`. Дом = Ф.18. | plan-104.10 Ф.3 → Ф.18 | остаток Ф.18 |
| `[M-104.2-symbol-cache]` | Каждый hover/goto re-parses файл; per-URI ResolvedModule cache (Module+file_map+env). | plan-104.10 Ф.1 | P1 |
| `[M-104.2-protocol-method-hover]` | Hover на protocol-method bodies не отдельно резолвится; cross-file hover + member-access. | plan-104.10 Ф.4/Ф.6 | P2 |
| `[M-104.2-signature-type-dispatch]` | ✅ **CLOSED Plan 104.10 Ф.8 (2026-07-03).** `signature_help.rs::compute_signature_help_in(resolved, src, pos)` ранжирует overload'ы: метод-вызов `recv.method(` → тип ресивера из `expr_types` (span.end == `.`) → `receiver_matches` даёт активный overload (dominant score); свободные fn — по арности под курсором. Server использует Ф.1-кеш (`get_or_build_resolved`). Graceful fallback на первый при неизвестном ресивере. 4 новых unit (t_pos1 receiver-dispatch, t_pos2 arity, t_neg1 unknown-fallback, t_edge1 nested `f(g(`)). | plan-104.10 Ф.8 | ✅ done |
| `[M-104.10-arg-type-dispatch]` | 🟡 PARTIAL (Ф.8). Свободные fn overload'ы ранжируются по *числу* аргументов (позиция курсора), не по инференс-*типам* уже введённых аргументов. Receiver-type dispatch (headline Ф.8) — полный; полная type-унификация аргументов свободных fn — follow-up. | plan-104.10 Ф.8 | P3 |
| `[M-104.2-expr-types-member-hover]` | Variant A (надмножество закрытого Variant B): `ModuleEnv.expr_types` opt-in per-expr type map → member-access hover (`r.start`), type-driven completion/signature. | plan-104.10 Ф.2/Ф.6 | P2 |
| `[M-104.6-symbol-table-rename]` | ✅ **CLOSED Plan 104.10 Ф.7 (2026-07-03).** `compute_rename` классифицирует символ под курсором из AST (`classify_scope`): local binding → rename ограничен телом объявляющей функции в primary-файле (`RenameScope::LocalInFile`); top-level fn/type/const → cross-file, но пропускает функции, локально затеняющие имя (`shadow_scopes_in_text`). Scope из реальных AST-биндингов (`collect_pattern_names`/`collect_block_bindings`/`collect_expr_bindings`), НЕ regex/brace-depth. Атомарный post-rename type-check (D296) сохранён. 7 новых unit (f7_pos local-scope/use-site/top-level/shadow-check/two-fns, f7_edge type-cross-file, f7_neg parse-degrade + no-cross-scope-false-positive). Остаток per-occurrence full-resolve → `[M-104.10-rename-full-resolve]`. | plan-104.10 Ф.7 | ✅ done |
| `[M-104.10-rename-full-resolve]` | 🟡 OPEN (Ф.7 остаток). Полный per-*occurrence* symbol-resolve (резолвить каждое вхождение до decl-Span и сравнивать) не сделан — нужен полный name-resolution pass, которого bootstrap-checker не отдаёт. Scope-модель over-approximate: local binding → вся объявляющая функция (не точный блок); shadow scope → вся затеняющая функция. Следствия (редкие, safe-by-omission): второй одноимённый биндинг в sibling-блоке той же функции переименовывается вместе; top-level ссылка ДО введения локали того же имени в той же функции консервативно пропускается. | plan-104.10 Ф.7 | P3 |
| `[M-104.10-file-rename-imports]` | 🟡 OPEN (Ф.18 остаток, план-санкц. «базовый + маркер»). `workspace/willRenameFiles` (`workspace_lifecycle.rs::compute_rename_import_edits`) переписывает import-путь по РЕАЛЬНЫМ AST import-span'ам: матч import, чей финальный сегмент == old stem И dotted-path — суффикс path-сегментов файла (root-agnostic, точно для single-file-module rename без запуска резолвера). НЕ покрывает: (a) peer-rename в folder-module (папка=модуль не меняется — правка не нужна); (b) коллизия leaf-имени в 2 несвязанных директориях (обе правятся — ambiguous без резолвера); (c) alias/selective re-spelling. Follow-up: resolver-verified path-matching. | plan-104.10 Ф.18 | P3 |
| `[M-104.10-watch-reverse-deps]` | 🟡 OPEN (Ф.18 остаток). `.nv`-watch-событие инвалидирует resolved-cache изменённого файла + всех открытых доков (`invalidate_all_resolved` — корректный superset, никогда не stale, грубее точного module-graph reverse-dep обхода; перестройка ленивая). Точный обратный граф — follow-up (пересекается с `[M-104.10-dependent-invalidation]`). Файл: `nova-lsp/src/{workspace_lifecycle.rs,state.rs}`. | plan-104.10 Ф.18 | P3 |
| `[M-104.10-typedef-implementation]` | ✅ **CLOSED Plan 104.10 Ф.19 (2026-07-04).** `nova-lsp/src/type_definition.rs` + хендлеры `goto_type_definition`/`goto_implementation` + capabilities. typeDefinition: тип под курсором из Ф.2 `expr_types` (innermost expr / let-binding init) → base-name → `type`-decl span → cross-file `Location`. implementation: идентификатор → AST-реестр (`#impl` opt-in ∪ структурная конформность для протокола; все `fn T @method` для метода), cross-file через `file_map`. 12 unit PASS (POS/NEG/EDGE typeDef + protocol/method/bound-use/cross-file impl). | plan-104.10 Ф.19 | ✅ done |
| `[M-104.10-typedef-ident-coverage]` | 🟡 OPEN (Ф.19 остаток, P3). typeDefinition на *использовании* идентификатора / generic-типе зависит от того, аннотировал ли Ф.2-чекер данный span в `expr_types`; где `[M-104.10-expr-types-coverage]` оставляет пробел (generic instance-chain returns, non-primitive TupleLit, несвязанный type-param) — typeDefinition graceful-None (тесты additive). Основной путь (let-binding → init-тип) под assert. Дом = Ф.2b full expr-walker. | plan-104.10 Ф.19 | P3 |
| `[M-104.2-body-walk-local-var-type]` | ✅ **CLOSED 2026-06-17 (Plan 104.2 Fix B).** body-walk обнаруживает курсор на `LetDecl`-биндинге и возвращает `SymbolInfo::LocalVar` с явной аннотацией типа из `LetDecl.ty`. Также: Fix A — `resolve_item` для `Item::Fn` возвращает `None` внутри тела → body-walk находит фактический callee (fn-body hover priority). | plan-104.2 Followups | ✅ done |
| `[M-104.2-local-var-type-inference]` | ✅ **CLOSED (Variant B, 2026-06-18).** `infer_rhs_type()` в `symbol.rs`: `ro r = 5` → `int`, `ro r = 0..=5` → `Range`, `ro r = foo()` → return type из `ModuleEnv.fns`. `check_module()` вызывается в `hover.rs` и передаётся в resolver. Вариант A (расширить `ModuleEnv.expr_types`) — followup для полной точности (Plan 104.3+). | plan-104.2 Followups | ✅ done |
| `[M-104.9-dynamic-method-completion]` | ✅ **CLOSED Plan 104.10 Ф.5 (2026-07-03).** Статические таблицы методов удалены; type-driven completion резолвит методы из `ResolvedModule` (expr_types → тип ресивера → scan `module.items`, вкл. inline stdlib + cross-file peers) в `completion.rs::method_items_typed`. | plan-104.10 Ф.5 | ✅ done |
| `[M-104.5-suggestion-field-wiring]` | CodeAction edit range: re-scan источника вместо compiler Suggestion.span. V2: wire напрямую. | plan-104.5 Followups | P3 |
| `[M-104.5-multi-edit-rename]` | E_PREFIX_SHADOWS_NAMED_TYPE: переименовать все вхождения, не только объявление. | plan-104.5 Followups | P3 |
| `[M-104.5-organize-imports]` | Sort+deduplicate import list (Source.organizeImports action). | plan-104.10 Ф.6 | P2 |
| `[M-treesitter-grammar-keyword-bump]` | **OPEN (2026-06-14).** tree-sitter-nova грамматика (github.com/nv-lang/tree-sitter-nova v0.1.0) отстаёт от лексера: anonymous-токены `unsafe` (D216), `priv`/`pub` (D220), `okdefer` (D160), `interrupt` НЕ определены в грамматике → Zed/Helix/Neovim `highlights.scm` не могут их подсветить (добавление в `.scm` без бампа грамматики ломает компиляцию query). Также грамматика, вероятно, ещё содержит retired `let`/`readonly`/`and`/`or`/`not`. **Regex-хайлайтеры (VSCode/vim/www) синхронизированы с лексером** (Plan 104.9, [D278](../../spec/decisions/09-tooling.md#d278); commits `ad55a91d` nova + `28b00c6` www) и защищены conformance-тестом `compiler-codegen/tests/syntax_highlight_conformance.rs` (8/8). Нужно: бампнуть грамматику (добавить/убрать токены+правила) → новый rev → обновить `editors/helix/languages.toml` `rev` + `editors/zed/languages/nova/highlights.scm` `@keyword` (тогда conformance-тест проверит и полноту Zed, а не только фантомы). Source-of-truth: `compiler-codegen/src/lexer/mod.rs`. | plan-104 Followups | P3 |
| `[M-118.1-addr-of-mut-deref-ptr-mut]` | ✅ **CLOSED (Plan 118.6, 2026-06-17):** `addr_of_mut` retired → `E_ADDR_OF_REMOVED`; `&x` на mut-биндинге автоматически даёт `*mut T`. | plan-118.1 Followups | ✅ done |
| `[M-118.1-addr-of-chains-checktime]` | ✅ **CLOSED (Plan 118.6, 2026-06-17):** `addr_of` retired → `E_ADDR_OF_REMOVED`; `&x.field` chain-check выполняется at check-time через escape analysis. | plan-118.1 Followups | ✅ done |
| `[M-118.6-tuple-field-escape]` | tuple field chain-root tracking — `&tuple.N` escape analysis. Escape analysis отслеживает корень для value-records и примитивов; кортежные поля (`tuple.0`, `tuple.N`) требуют отдельного chain-root-tracking в escape_analyze.rs. | plan-118.6 Followups | P3 |
| `[M-118.1-ffi-perf-bench]` | memcpy/memmove bench harness для FFI intrinsics не построен (сами intrinsics landed). | plan-118.1 Followups | P2 |
| `[M-test-runner-large-test-lane]` | ✅ **DONE (Plan 156, suffix-only механизм, 2026-06-14).** ТРЕБОВАНИЕ выполнено: дефолтный `nova test`/CI быстрый (компиляция И выполнение). Lane реализован как **per-file суффикс `_slow.nv`** (зеркало `_windows.nv`/`_test`/`_module.nv`): skip на этапе discovery в `walk_nv_filtered` → нулевой per-file I/O (содержимое не читается), `SlowLane{Exclude(default)\|Include\|Only}` + clap-флаги `--include-slow`/`--slow-only`; Rust unit-тесты discovery (`plan156_slow_lane_tests`); plan156 фикстуры; генератор `nova-codegen unicode --conformance-full` пишет `*_conformance_slow.nv` (без cap). Нормировано [D376](../../spec/decisions/09-tooling.md#d376-test-discovery-skiproute-конвенции--fixtures-os-суффикс-_slownv). **rev-3 (2026-06-15):** полные `*_conformance_slow.nv` корпуса **НЕ коммитятся** — gitignored, **регенерируются on-demand** из pinned UCD (модель Go/CPython; cross-eco research → `docs/research/10-unicode-test-data-storage.md`); коммитится только fast-сэмпл. Populate-фаза workflow доказала полную генерацию (UCD 16.0 докачан, collation 227800/227800 сгенерён, sentence/word/grapheme прогнаны `--slow-only`=PASS), затем файлы выкинуты из истории (rebase-drop ДО мержа). Каталог-вариант `slow/`+сентинел отложен → `[M-156-slow-subtree-dir]`. CI slow-gate (регген+`--slow-only`) → `[M-152-collation-full-conformance]`. См. `docs/plans/156-test-runner-slow-lane.md`. | Plan 156 | ✅ DONE |
| `[M-codegen-conformance-stack-overflow]` | ✅ **FIXED 2026-06-15 (Plan 158, ветка plan-cgstack).** `nova-codegen test-all` падал **`thread has overflowed its stack`** (exit 127) на больших conformance-фикстурах (`normalization_conformance`, ~6000 asserts). **Корень (разведка):** НЕ рекурсивный codegen — тот же файл через `test-build` (главный поток, 8 МБ стек) **проходит exit 0**; падали только **worker-потоки** test-all (`thread::scope` `s.spawn` с дефолтным ~2 МБ стеком). Регрессия дельты `2eb59b04..b0095867` (153.x codegen стал чуть глубже/тяжелее по стеку). **Fix:** `Builder::new().stack_size(64 MB).spawn_scoped(s, …)` в `test_runner.rs` (НЕ band-aid — codegen-глубина нормальна на обычном стеке, недосайзен был только worker-стек). Верифицировано релизным бинарём: `normalization_conformance` через test-all → PASS (был 127); plan152_4 15/1 где 1 = flaky `lld-link: cannot open output file` (AV-гонка, не регрессия, уходит с `--retries`). recursion→iteration рассмотрен и отклонён (огромный риск ради патологии >64МБ-глубины). См. docs/plans/158-test-runner-worker-stack.md. | Plan 158 | ✅ DONE |
| `[M-156-slow-subtree-dir]` | **OPEN (отложено, Plan 156 rev-2 out-of-scope).** Папка `slow/` (без `_`, зеркало `fixtures/`) + опц. сентинел `_slow.toml` (зеркало `_fixture.toml`) для случая «медленный **folder-module** из ≥2 peer'ов» — когда суффикс `_slow.nv` неудобен (каждый peer пришлось бы суффиксить). YAGNI: такого теста нет, conformance-корпуса = одиночные файлы. Добавляется аддитивно (новый `is_slow_dir` рядом с `is_fixture_dir`), не ломая suffix-механизм. Нормировать в D-блоке `09-tooling.md` вместе с `fixtures/`/`_fixture.toml`. | Plan 156 | P3 |
| `[M-152-case-fold]` | ✅ **CLOSED 2026-06-14 (Plan 152.4.4).** `fold_case(s)` + полные locale-independent Unicode `to_uppercase`/`to_lowercase(s)->str` (multi-cp: ß→SS, ﬁ→FI, İ→i̇) + **Final_Sigma** context (Σ→ς/σ через Cased/Case_Ignorable). Данные: `nova-codegen unicode` → `case_data.nv` (FOLD/LOWER/UPPER maps из CaseFolding C+F + SpecialCasing unconditional + UnicodeData[12,13]; CASED/CASE_IGNORABLE ranges из DerivedCoreProperties). G0 (2 слоя): breadth-conformance **2981/2981** out-of-band (коммит uniform-spread 1500) проверяет парсер+lookup+emission; **independent UCD hand-oracle** (`case.nv`) пиннит выборку (Turkic-excl, field-index, 3-cp, Final_Sigma+case-ignorable). plan152_4 10/10. ASCII `transform.nv` `to_upper/lower` оставлены ASCII-only (opt-in: Unicode-версии в `std/unicode`). Title-casing вынесен → `[M-152-word-boundaries]`. | Plan 152.4.4 | ✅ DONE |
| `[M-152-word-boundaries]` | ✅ **CLOSED 2026-06-14 (Plan 152.4.5).** `str.@as_words() -> WordsView` (4-я линза, UAX #29 WB1-WB16: lookahead WB6/7/11/12 + WB4 ignore-rule + RI-parity WB15/16) над `word_data.nv` (WordBreakProperty range-таблица; ExtPict для WB3c reused). **`to_titlecase(s)->str`**: титулкейс первой cased-буквы слова (TITLE-маппинг, не upper — ǆ→ǅ) + lowercase остального (Final_Sigma). G0: **полный WordBreakTest.txt 1826/1826** (content-checked, **INDEPENDENT** UCD-oracle) out-of-band; коммит uniform-spread 1500. plan152_4 13/13. RI-parity инкрементальна O(n) (review-фикс). TITLE-маппинг добавлен в `case_data.nv` (UnicodeData[14]+SpecialCasing title). Sentence-сегментация → `[M-152-sentence-boundaries]`. | Plan 152.4.5 | ✅ DONE |
| `[M-152-sentence-boundaries]` | ✅ **CLOSED 2026-06-14 (Plan 152.4.6).** `str.@as_sentences() -> SentencesView` (5-я линза, UAX #29 **SB1-SB11 + SB998**: SB5 Extend/Format ignore-rule + ATerm/STerm Close* Sp* контекст-стейт-машина + SB8 forward-lookahead до Lower/blocker) над `sentence_data.nv` (SentenceBreakProperty range-таблица, 14 категорий, 2898 ranges). Дефолт **НЕ ломать** (SB998 = ×), противоположно grapheme/word. `SentencesView` (`value priv {buf,bounds,idx}`) — `Next[str]` + `iter`/`count`(remaining)/`is_empty`, O(n) на создание. G0: **полный SentenceBreakTest.txt 512/512** (content-checked, **INDEPENDENT** UCD-oracle) — коммит = весь файл (512 < лимита 1500). plan152_4 16/16. `Mr. Smith`=3 сегмента (дефолтный UAX #29 без словаря аббревиатур — **по дизайну**). Попутно codegen-баг: line-wrapped `\|\|`-цепочка в теле fn → мис-лоуэр в closure, захват module-`const` как C-локалов → fix однострочным `=>` телом (`[M-codegen-line-wrapped-or-chain-closure]`). | Plan 152.4.6 | ✅ DONE |
| `[M-152-collation]` | ✅ **CLOSED 2026-06-14 (Plan 152.5b).** DUCET collation (UCA/UTS #10) — `std/unicode/collate.nv` (opt-in): `collate_compare`/`collate_sort_key`/`collate_eq`/`Collator`. NFD → collation elements (longest-match contractions **+ S2.1 discontiguous** + implicit-веса CJK/Tangut) → multi-level **Shifted** sort key → лекс-сравнение. `collate_data.nv` из DUCET `allkeys.txt` (`nova-codegen unicode`; 38443 single/77 contraction/21 implicit). G0: `CollationTest_SHIFTED.txt` (INDEPENDENT oracle) **50000/50000 spread out-of-band** (коммит 1500). S2.1-фикс закрыл 4/200 краевых чанка (combining-mark вклинивается в контракцию). Codegen-баг попутно: match-as-return c `mut found=None`→bool (workaround typed `Option[str]`). | Plan 152.5b | ✅ DONE |
| `[M-152-collation-tailoring]` | **OPEN (Phase B).** CLDR locale-tailoring (`Collator` с локалью, аналог ICU tailored / JS `localeCompare` с locale). DUCET root-collation (152.5b) уже есть; tailoring — отдельный слой (CLDR-данные + tailoring-rules). **UPD 2026-06-16:** `str @eq_ignore_case` (тонкий враппер над `fold_case`) ✅ DONE (коммит `fab43385`, D254 §eq_ignore_case ЗАКРЫТ) — выделен из этого маркера; остаётся только CLDR-tailoring. | Plan 152.5 (Phase B) | P2 |
| `[M-152-unicode-codepoint-u32]` | ✅ **CLOSED (Plan 152.8, 2026-06-16).** Слой 1: Vec[int]→Vec[u32] во всех range-таблицах std/unicode (gcb_flat, extpict_flat, incb_flat, gc_flat, alpha_flat, ws_flat, cased_flat, caseign_flat, sb_flat, wb_flat, implicit_flat) + codepoint-буферы. Vec[int] намеренно оставлены: offs/bounds/starts (byte offsets), cats (category codes), used/consumed (collation indices). Слой 2: nova_char int64_t→uint32_t (D128 AMEND) в nova_rt.h + gc_layout.rs (4,4) + emit_c.rs (U-suffix char literals). Fix: 57df9648 (category.nv) + 97934a57 (case.nv). 6/6 PASS plan152_8 + 9/9 PASS plan152_7 via main nova.exe. | Plan 152.8 | ✅ |
| `[M-152-collation-full-conformance]` | **OPEN (CI slow-gate).** Slow-lane СУЩЕСТВУЕТ (Plan 156 ✅), генерация полного 227800-пар `collation_conformance_slow.nv` ДОКАЗАНА (Populate-фаза: UCD 16.0 докачан, `--conformance-full` → 227800/227800). **rev-3:** файлы НЕ коммитятся — регенерируются on-demand. Остаток = **CI-обвязка**: настроить slow-gate (merge-to-main + nightly), который (a) фетчит/имеет pinned UCD, (b) прогоняет `nova-codegen unicode --emit-conformance --conformance-full --ucd-dir <UCD>` в gitignored-кэш, (c) `nova test --slow-only --timeout 600` как доказательство G0. Сейчас G0 = 50000-spread out-of-band + 1500 fast-коммит. НЕ data-gate (данные доступны через генератор), а CI-инфра. | Plan 156 + 152.5b | P3 |
| `[M-152.3b-char-unicode]` | ✅ **DONE 2026-06-15 (Plan 152.3b).** Unicode-aware `char`-методы (opt-in `std.unicode`): `@is_alphabetic`/`@is_numeric`/`@is_alphanumeric`/`@is_whitespace`/`@is_uppercase`/`@is_lowercase`/`@is_control`/`@general_category()->GeneralCategory`/`@to_uppercase()`/`@to_lowercase()->str` (multi-cp: ß→SS, ﬁ→FI, İ→i̇, lone Σ→σ). Делегируют в новую `category.nv` поверх сгенерированной `category_data.nv` (General_Category + Alphabetic + White_Space, UCD 16.0; `is_numeric` из GC Nd|Nl|No). 1:1 с UCD, семантика == Rust `char`. Методы в `category.nv` (НЕ prelude/`defaults.nv` — иначе тянет таблицы в каждую программу). D252 Unicode-часть → IMPLEMENTED; Q-char-case-return-type (str vs итератор → **str**). plan152_3 4/0. Попутно закрыл `[M-codegen-conformance-stack-overflow]` (64 MiB worker-стек). | Plan 152.3b | ✅ DONE |
| `[M-152.3b-category-full-conformance]` | **OPEN (CI slow-gate, аналог `[M-152-collation-full-conformance]`).** `category_conformance.nv` коммитится как **sampled** (fast-регресс). Полный per-code-point прогон General_Category/Alphabetic/White_Space по всему диапазону U+0000..U+10FFFF — slow-lane (Plan 156, регенерация on-demand из pinned UCD: `nova-codegen unicode --emit-conformance --conformance-full --ucd-dir <UCD>` → `*_conformance_slow.nv` → `nova test --slow-only`). Также `nova-codegen unicode --check` против полного UCD (локальный `tmp_ucd` без `PropList.txt`) — CI-guard slow-gate. НЕ упрощение механизма (1:1-с-UCD полный), а CI-обвязка/data-availability. | Plan 156 + 152.3b | P3 |
| `[M-152.3b-char-methods-no-import]` | ✅ **CLOSED 2026-06-15 (Plan 159 Ф.4, Option A).** Unicode char-методы (`'Ω'.is_alphabetic()`, `is_uppercase`, `general_category`, `to_uppercase`…) теперь работают **БЕЗ `import std.unicode`** (как Rust `char::is_*` inherent). Механизм: import-резолвер детектит char-Unicode **method-call**-селектор (`expr.foo()`, отличён от bare-free-fn формы новым `@method:`-тегом в `lints::collect_expr`) и инжектит `import std.unicode` в **пользовательский entry-модуль** (НЕ в prelude-фасад) — обычный cycle-free путь, поэтому цикл `prelude→unicode→collections→prelude` (stack overflow) НЕ входится. Plan 159 Ф.1 reachability-DCE затем срезает все неиспользуемые Unicode-таблицы → no-import стоит **ноль** для программ, не вызывающих эти методы (single-method no-import проба = байт-идентичный DCE-профиль explicit-import: 10606→2494). Bare free-function вызовы (`general_category(0x41)`) остаются opt-in за `import` (пин plan152_3/n_char_unicode_opt_in.nv). Изменения аддитивны (+170 строк, 0 удалений; без edit prelude.nv, без eager-import). Полная lazy-module-resolution НЕ потребовалась → `[M-159-lazy-module-resolution]` (P3). | Plan 152.3b / 159 | ✅ DONE |
| `[M-reachability-codegen-dce]` | ✅ **Ф.1-core DONE 2026-06-15 (Plan 159, D283).** Reachability-codegen (вариант A, Zig-модель) ЗАШИПЛЕН Ф.1–Ф.4 (ветка `plan-159-reachability-impl`, НЕ merged). Codegen эмитит в C только достижимое от `main` (free fns + module-level `const` + `ro` lazy-static globals + методы), worklist-обход + засев непрямых/desugar-селекторов, kill-switch `NOVA_REACH_DCE` (unset/`!=0` ⇒ ON default; `0` ⇒ байт-идентичное старое поведение). Executable: `export`/`pub` НЕ roots (нет C-ABI-экспорта); FFI-entry = только `is_external`. Library / no-`main` / `EXPECT_CC_ERROR` ⇒ DCE OFF (полнота API). Замер: `import std.unicode.{is_alphabetic}` + вызов только `is_alphabetic` → C **10606→2494** строк (~4.25×↓), collate/normalize/GC_DATA 37/9/2→**0/0/0**, нужная ALPHA_DATA сохранена, PASS. Acceptance A1-A5+G0 MET (G0 консервативная корректность: никогда не отрезать достижимое). dce_tests 21/0; per-area A/B vs `NOVA_REACH_DCE=0` zero NEW FAIL. Method-DCE coarse-by-name (over-keep на name-collision). Ф.4 (Option A) разблокировал no-import char-методы → `[M-152.3b-char-methods-no-import]` ✅ CLOSED. Остаток (оба over-keep, не корректность): `[M-159-method-pruning]`, `[M-159-lazy-module-resolution]` (см. Q-reach-dce-precision). Вариант B (прекомпил std + `cc --gc-sections`) — отдельная задача под скорость сборки. Research: docs/research/11-stdlib-method-resolution-reachability.md; план: docs/plans/159-reachability-codegen.md. **2026-06-15 (после воркфлоу): полный регресс (10 батчей, PASS=2738/FAIL=172/SKIP=61) + kill-switch A/B по всем 51 падавшим dir** (main-бинарь был устаревшим → невалиден; единственная валидная база = `NOVA_REACH_DCE=0` на том же бинаре/фикстурах). DCE-ON FAIL ≤ DCE-OFF FAIL везде — все падения pre-existing. **Найден+починен 1 РЕАЛЬНЫЙ over-prune** (commit b26d1cab): `collect_used_names` не обходил `Contract.message_expr` → method-DCE срезал int→str-конвертер интерполяции в сообщениях контрактов (`requires`/`ensures`/`invariant "...${f}..."`) → `nova_str`←`int` CC-FAIL. Fix аддитивный (over-keep). Guard: plan159/f2_contract_msg_interp_dce.nv + plan140_1/invariant_msg_interp_neg. Смёржено в main (merge main→ветка 8e438df7 + FF). **Ф.1-Ф.4 DONE+MERGED.** | Plan 159 | ✅ DONE+MERGED |
| `[M-159-method-pruning]` | **OPEN (Plan 159 followup, P3).** Method-level DCE — **coarse-by-name**: метод `T.m` режется только если **И** тип-имя `T`, **И** селектор `m` недостижимы (пересечение по имени, не per-(тип,метод)-ребро). Name-collision (достижимый `A.foo` + dead `B.foo`) **over-keep'ит** `B.foo` — корректно (G0), но не максимально точно. Точная альтернатива = настоящий call-graph + monomorphization-collector (rustc-модель); её риск — пропустить codegen-injected (desugar) селектор и **over-prune** (G0-blocker). Текущий список засеянных desugar-селекторов (for-iter/операторы/concat/index/string-interp) собран РЕГРЕССИЕЙ (concat нашёлся только sweep'ом plan131), не систематическим перечнем. Задача: per-kind аудит редких инъекций (drop/finalizer, embed auto-proxy, closure-captured методы) + фикстура на каждый вид, ИЛИ переоценить нужность точного method-body DCE. Делать при заметном over-keep на реальной программе. Q-reach-dce-precision §1. | Plan 159 | P3 |
| `[M-159-lazy-module-resolution]` | ✅ **CLOSED 2026-06-16 ([Plan 162](162-rust-model-module-resolution.md)).** Rust-модель module-resolution реализована: cycle-guard (`in_progress` visited-set → `return Ok(())`), TypeMethodMap (inherent методы без import), char Unicode-методы в prelude/core.nv, `CHAR_UNICODE_METHOD_SELECTORS`+`needs_unicode_injection` удалены, extension-метод → `E_EXTENSION_METHOD_NEEDS_IMPORT`. Межмодульные циклы разрешены. D285-D287. | Plan 162 | ✅ DONE |
| `[M-159.1-onexit-drop-overprune]` | ✅ **CLOSED 2026-07-16 ([Plan 159.1](159.1-method-reachability-dce.md) Ф.1).** P0-риск аудита 2026-07-16: `consume X = e { … }` scope-exit `@cleanup` dispatch (`Nova_<T>_consume_cleanup`, синтетический символ, `.cleanup(…)` никогда не пишется в исходнике) мог тихо резаться method-DCE. **Найдено: seed (`out.insert("cleanup")`, `Stmt::ConsumeScope` arm, `lints.rs:1015`) уже был добавлен коммитом `70c4eff02` (2026-07-15, `[M-187-tls-cross-pkg-consume-cleanup]`) — ДО этого аудита** (урок §7.7 повторился). «Рекурсия по droppable-полям» проверена — НЕ требует отдельного кода (диспозиция всегда явный AST Call/ConsumeScope, уже покрыто). Волна добавила недостающий permanent regression-guard (`70c4eff02` верифицировался ad-hoc, без теста): 2 Rust unit-теста (`emit_c.rs::dce_tests::consume_scope_exit_seeds_cleanup_method` + `..._for_nested_consume_field`) + 2 E2E `.nv`-фикстуры (`nova_tests/plan159_1/`, `nova_tests/plan159_1_nested/`, legacy `EXPECT_RUNTIME_PANIC`+`fn main`, `--panic` selector). RED→GREEN эмпирически подтверждён (toggling сида) на Rust-unit-test уровне И E2E (link-level `undefined symbol` без сида → сид восстановлен → PASS). | Plan 159.1 | ✅ DONE |
| `[M-import-glob-forbid]` | ✅ **CLOSED 2026-06-16 ([Plan 163](163-import-export-glob-hygiene.md) Ф.4 amend).** V1: `E_REEXPORT_GLOB` (Ф.1) + `E_IMPORT_GLOB` (Ф.2, вариант a: запрет) + ~100 файлов `import X as X`. Amend: `E_IMPORT_GLOB` убран → `import m` легален (last segment = qualified name); `E_REDUNDANT_IMPORT_ALIAS` добавлен (`import a.b.X as X` запрещён); ~123 файла мигрированы `import X as X` → `import X`. D288+D289 amend. | Plan 163 | ✅ DONE |
| `[M-lazy-static-thread-safety]` | **OPEN (Plan 152.4 foundation).** Module-level `ro` lazy-static globals (`emit_lazy_const`) используют first-touch init **без синхронизации** (`if (!_init) { _value = build(); _init = 1; }`). При конкурентном первом доступе из нескольких fiber'ов/потоков — race (двойной build + torn `_init`). Не хуже существующего non-constexpr `const` lazy-init (та же модель) — не новый класс. Fix: `pthread_once`/atomic-CAS guard, либо eager-init на старте для prelude-критичных. Unicode-таблицы сейчас обычно тригерятся из main до spawn. | floating | P3 |
| `[M-152.7-write-sink]` | ✅ **CLOSED 2026-06-16 (Plan 152.7.1, commits `a313926b`+`3d0e30fa`).** `Write` протокол `{ mut @write_str(s str) -> () }` введён; Display/Debug мигрированы `sb StringBuilder`→`w Write` во всех impl (int/f64/f32/bool/char/str/Vec + auto-derive); `StringBuilder.@write_str` impl добавлен; codegen `Write`→`Nova_StringBuilder*` (static mono); интерполяция не сломана. plan154_1 9/0, plan137 16/0, plan91_14 14/0, plan126 21/0. D258 AMEND + D183/D229 AMEND. | Plan 152.7.1 | ✅ done |
| `[M-interp-unsupported]` | **OPEN (floating, ветка `chore-disable-interp-nova-run`, worktree nova-noninterp).** Интерпретатор (tree-walker) ОТКЛЮЧЁН: `nova run` оставлен видимой CLI-командой, но при вызове печатает явное «interpreter not currently supported» и направляет на `nova build` / `nova test` (C-codegen) — единственный поддерживаемый путь. Сделано: (a) `nova-cli/src/main.rs` — `run` теперь error-stub (не вызывает интерпретатор); (b) `compiler-codegen/src/interp/mod.rs` — module-note «currently unsupported»; (c) удалены DEAD interp-тесты, ссылавшиеся на снятый `nova` interpreter-крейт (`nova-cli/tests/run_interp_named.rs`, `compiler-codegen/tests/{spec_nova.rs,integration.rs}` interp-части + `tests/common` хелперы); (d) user-facing доки + www site вычищены от `nova run` (README/.ru + examples + сайт `be06628`); (e) nova-cli доки (`docs/nova-cli.md`/.ru.md) выверены против реального CLI; (f) регресс-тест `nova-cli/tests/interp_unsupported.rs` (negative `nova run` ошибается + positive `nova check` работает, прогон через релизный бинарник, 2/2 PASS). **Plan [157](157-interpreter-unsupported.md) ✅ DONE · spec [D274](../../spec/decisions/08-runtime.md) · [Q-interpreter-future](../../spec/open-questions.md) ✅ RESOLVED (2026-06-14).** **Internal-tool residual DONE (2026-06-14, ветка `chore-disable-interp-codegen-tool`, worktree nova-interp2):** `nova-codegen run`/`test-interp` застаблены — handlers больше не конструируют `interp::Interpreter`, громко ошибаются (exit ≠ 0) с указанием на C-codegen, clap doc-строки `[UNSUPPORTED]` (commit `0d7116f4`); `compile`/`check`/прочее работают; `docs/nova-codegen.md`/`.ru.md` выверены (тот же commit); регресс `compiler-codegen/tests/interp_tool_unsupported.rs` neg run/test-interp + pos compile 3/3 PASS, build green (commit `a4e26525`). **Residual/future (единственный, не блокирует):** полное удаление модуля `interp/` ЛИБО его оживление как REPL — сознательно ОТЛОЖЕНО (`interp/` оставлен «для справки», consistent с «пока»/D274). | floating (interp removal / REPL) | P3 |

## P3 — Module system

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-json-serializer-set-pending-naming]` | **OPEN 2026-07-22 (аудит владельца).** `fn JsonSerializer mut @set_pending(key str) -> Result[(), SerError]` (std/src/encoding/serde/json.nv:71) — `set_`-префикс нарушает property-конвенцию (D117/D84/D409: сеттер = `mut @x(v) -> @`). Нюанс: fallible (`-> Result`, не `-> @`) → это НЕ чистый property-сеттер, а реализация протокол-метода `@struct_field(key)` (serde.nv:136). Fix: инлайн в `@struct_field` ИЛИ императивное имя без `set_`. Мелочь std-гигиены. | serde std / naming | **P3** |
| `[M-folder-module-detector]` | `is_folder_module_peer` (`imports.rs:1714`) — лишняя функция. `check_module_path` может определить тип модуля из декларации: `decl.last() == folder_name` → пир, иначе single-file. Текущий workaround: убрано ограничение `entries.len() < 2`, добавлена проверка `decl.last() == folder_name`. Правильное решение: удалить `is_folder_module_peer`, логику встроить в `expected_module_path_rev3`. | floating (manifest.rs / imports.rs) | P3 |

## P3 — Codegen cleanliness (генерируемый C полиш; рантайм не затронут)

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-codegen-dead-erased-generic-stubs]` | Type-erased `Vec[any]` (prelude-вариадик) эмитит NULL-stub методы — DCE. | codegen-cleanup mini-план | P3 |
| `[M-codegen-unit-block-temp-elision]` | `unit`-block-expr в discard-позиции → бессмысленный `_nv_tmp`. | codegen-cleanup mini-план | P3 |
| `[M-codegen-fluent-tail-if-unify]` | ✅ **CLOSED 2026-06-14 (D275, ветка `plan-cgfix-fluent-tail-if`).** Fluent-метод (`-> @`, `Vec.push`/`StringBuilder.append`) как **хвост ветки `if`/`match`** типизировался в C как `Vec*`/`Builder*`; при не-расходящемся `unit`-соседе в discard-позиции → `tmp(Vec*) = NOVA_UNIT;` (C-несовпадение) → CC-FAIL, ломало компиляцию всего `std.unicode` (peer-модуль). `emit_match` коэрсил (unit-доминирование `[M-91.13]`), `emit_if_expr` — нет. **Fix** (`compiler-codegen/src/codegen/emit_c.rs`): (1) `emit_if_expr` (~25905) — симметричный `(else_diverges, else_ty)` для обеих форм else + gated unit-доминирование (gated на `chosen != nova_unit` и не-расходящийся сиблинг → Plan 125 сохранён); (2) `infer_expr_c_type(Match)` (~34643) — то же `any-unit-arm → nova_unit`, что и `emit_match` (infer↔emit симметрия, иначе `if {push} else {match}` рассинхрон). **Workaround УБРАН** из `std/unicode/case.nv` (прямой fluent-стиль восстановлен; critério «без упрощений как для прода» выполнен). Acceptance: `cgfix_fluent_tail_if` 1/1 + `plan152_4` 13/13 + 0 новых регрессий (vs merge-base `22aa4944`). Коммиты `ef6d570a`+`f9c6e372`+`0f0a65a8`. | D275 | ✅ DONE |
| `[M-codegen-src-synthesized-attribution]` | `/* SRC */` только statement-granular; синтезированный C без атрибуции. | codegen-cleanup mini-план | P3 |
| `[M-codegen-short-freefn-name-collision]` | Free-функция с коротким именем `wb` (std.unicode/words.nv) дала каскад `undefined identifier wb` при компиляции — коллизия с runtime-локалом `wb` в `write_buffer.nv` (генерится в C как `wb` в каждом юните). Спаны указывали в peer-файлы (merged-буфер модуля). Workaround: переименование `wb`→`wbcat`. Fix: codegen должен мэнглить/скоупить имена free-функций так, чтобы они не сталкивались с C-локалами runtime-хелперов. Найдено в Plan 152.4.5. | codegen-cleanup mini-план | P3 |
| `[M-codegen-line-wrapped-or-chain-closure]` | Длинная `\|\|`-цепочка булевых сравнений, **перенесённая на 2 строки** в теле free-fn (block-body `{ a == X \|\| ... \n \|\| ... }`), мис-лоуэрилась в codegen в **closure-захват**: генерился `nova_lambda_*_env` со ссылками на module-`const` (`SB_LF`/`SB_CR`/…) как на **C-локалы**, которых в скоупе функции нет → каскад `use of undefined SB_LF` во всех юнитах, импортирующих модуль (peer merged-buffer). Workaround: однострочное `=>`-тело (`fn sb_sb8_blocker(c int) -> bool => c == X \|\| … \|\| Y`). Подозрение: парс/лоуэр многострочного `\|\|` после первого operand'а интерпретирует продолжение как closure-материал. Найдено в Plan 152.4.6. Fix: codegen не должен синтезировать closure для чисто-выражательного многострочного `\|\|`-тела. | codegen-cleanup mini-план | P3 |
| `[M-codegen-value-type-generic-forward-decl]` | ✅ CLOSED Plan 165 (2026-06-16). Generic value-тип `VecIter[T] value` forward decl bug исправлен (commits `1f92f106` `e7094f97`): теперь `type_ref_to_c` обходит `type_aliases` для generic value-типов с explicit type args → `NovaValue_VecIter____nova_int`. Range literal stack-init (`ExprKind::Range` + `infer_expr_c_type`) фиксирован аналогично. Для будущих generic value-типов: проверить forward-decl mono-имя (D290 §4). | codegen-cleanup mini-план | P2 closed |
| `[M-method-resolution-registry-inconsistency]` | codegen держит два method-реестра с **противоположным** tie-break: `method_receivers` (single-key, last-wins) vs `method_overloads` (multi-key Vec, first-match). Безвреден после Plan 154 (override чужого метода = `E_METHOD_REDEFINITION`, D267), но дублирование+рассинхрон стоит унифицировать. Найдено при Plan 154 investigation. **Частично адресовано Plan 152.4.3 (`738b6c2e`):** return-type inference (`infer_expr_c_type` fallback) теперь предпочитает type-qualified `fn_ret_<Type>_<m>` вместо name-only last-wins — починило коллизию `CharsIter.next`/`GraphemesView.next`. Остаётся: унификация самих dispatch-реестров (method_receivers vs method_overloads). | codegen-cleanup mini-план | P3 |
| `[M-codegen-blanket-generic-param-order]` | ✅ **CLOSED (Plan 164 Ф.2, 2026-06-16, commit `b94d46f3`).** Root cause: `emit_call` и `infer_expr_c_type` использовали `fn_decl.generics.first()` для поиска receiver typevar в blanket-fn. При порядке `[T Compare, I Next[T]]` биндился T→concrete_iterator (неверно), I оставался нераспознанным → сломанное mono C-имя. Fix: заменить `generics.first()` на `generics.iter().find(|g| g.name == type_name)` где `type_name` — registered receiver typevar из `method_receivers`. Два сайта в `emit_c.rs`: emit_call (~25382) + infer_expr_c_type (~35277). Fixture `plan164/blanket_param_order.nv` 5/5 PASS. | Plan 164 Ф.2 | ✅ done |
| `[M-uint-legacy-array-uint64-until-a4]` | ✅ **CLOSED (Plan 172.12 A7+A8, 2026-07-07/08, коммиты `12415f…`-era + `0292f3694`/`65d165e75`/`99b3d2ce2`).** A7 закрыл binding/ctor/receiver пути (`[]uint` → `Nova_Vec____nova_uint`), A8 добил infer-твины и снёс сам `NOVA_ARRAY_DECL/IMPL(uint64_t)` из array.h — legacy-путь физически не существует. Исходное описание: Legacy `NovaArray_<T>` путь (`[]uint.new()/.push()`, 5 element-key арм в `emit_c.rs`) эмитит `NovaArray_uint64_t` при каноне `uint = nova_uint = uintptr_t` (Plan 133). ABI-идентично ТОЛЬКО на 64-бит (`nova_uint`≡`uint64_t`, layout `{ptr; nova_int len; nova_int cap}` совпадает). Слепой ренейм в `nova_uint` ломает линковку: `nova_rt/array.h` физически содержит `NOVA_ARRAY_DECL(uint64_t)`, НЕ `nova_uint` (→ `unknown type NovaArray_nova_uint`); а runtime-`NOVA_ARRAY_DECL(nova_uint)` ещё и переопределил бы **compiler-emitted mono `NovaOpt_nova_uint`** (Vec-flip путь эмитит его сам). Проверено (2026-07-07): дуальность `NovaArray_uint64_t` (static-call) vs `Nova_Vec____nova_uint` (Vec-flip `.of()`/type-pos) сосуществует в ОДНОМ CU БЕЗ клэша — incompatible-pointer-**name**, но layout-идентичный → компилит+линкует+рантайм-корректно; pre-existing (baseline-identical), класс `[M-array-vec-unify]`. Закрытие: ЛИБО снос legacy element-key арм (тогда `.new()` тоже пойдёт Vec-flip → `nova_uint`), ЛИБО ренейм typedef в `nova_rt/array.h` (`NOVA_ARRAY_DECL(nova_uint)` + разведение с mono-`NovaOpt_nova_uint`) + потребители. **Переисследовано в A4 (заход 8, 2026-07-07): блокер подтверждён неизменным** (`array.h` физически без `NOVA_ARRAY_DECL(nova_uint)`); `receiver_c_type`'s single-level `[]T` путь однородно обрабатывает ВСЕ примитивы одной legacy-веткой — точечный uint-фикс создал бы асимметрию с соседями (int/str/bool/…), не локальный патч. Оба варианта закрытия — cross-cutting runtime+codegen работа с широким test-surface, тот же root cause что `[M-array-vec-unify]` → **переадресовано в 172.12 A5** (закрывается вместе, не как отдельный pre-A5 патч). | Plan 172.12 A5 | P3 |
| `[M-dead-exprkind-blocking-vestigial]` | `ExprKind::Blocking(Block)` (ast/mod.rs, ~2558) — мёртвый AST-вариант: Plan 113/91.15 (D172) ретрактнул `blocking { }` блок-форму, парсер (`parse_blocking`) теперь ВСЕГДА возвращает `[D172-block-form-removed]` parse-error — конструктор `ExprKind::Blocking` больше никогда не производится. Но арм остаётся во ВСЕХ match'ах, что его обходят (capability-walker в `types/mod.rs`, вероятно codegen/verify) — неполная ретракция, dead-code-хвосты. Найдено при аудите capture-check D415 §2 (Plan 173.3 detach-amendment, 2026-07-11): `#blocking fn`-атрибут (живая замена) не имеет lexical-capture поверхности (fn-item, не closure) — сам факт что пришлось это доказывать через мёртвый арм подсветил хвост. Снос: удалить variant из `ExprKind` + все match-армы, что его обходят (перепроверить codegen/verify на использование). Широкий диф (много файлов матчат `ExprKind::*` exhaustively) — не блокер, чистка на досуге. | codegen-cleanup mini-план | P3 |

## P3 — Docs / Sugar

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-169.1-timing-report-regression-gate]` | ✅ **CLOSED**: `--max-test-ms N` флаг — тесты превышающие порог → список + exit 3. | Plan 169.1 | ✅ done |
| `[M-169.1.1-lane-flags]` | ✅ CLOSED 2026-06-19 — TestSelection + CLI flags (Ф.1) | Plan 169.1.1 | ✅ done |
| `[M-169.1.1-ci-workflow]` | ✅ CLOSED 2026-06-19 — nova-test-regression.yml (Ф.2) | Plan 169.1.1 | ✅ done |
| `[M-118.1-typed-pointer-cookbook]` | docs/typed-pointers.md cookbook не написан (есть только Plan 115 FFI cookbook). | plan-118.1 Followups | P3 |
| `[M-118.1.7-extern-block]` | `extern "C" { unsafe fn … }` block-сахар (gated на multi-ABI); сейчас individual `external unsafe fn`. | plan-118.1 Followups | P3 |
| `[M-D227-alias-newtype-range]` | D227 range-check НЕ покрывает alias/newtype над sized-int (`assignable()` чекает только direct Named + Readonly/Mut/Unsafe; резолв alias-имени требует `self.types`, недоступного на free-fn coercion-сайте). | plan-142 Scoped open-questions | P3 |
| `[M-D227-float-range-check]` | D227 Rule 5 (f32 exponent overflow) НЕ реализован; Ф.1 scope был integer-only (8 sized-int). | plan-142 Scoped open-questions | P3 |
| `[M-labeled-loops]` | Метки циклов + адресный `break outer`/`continue outer` (выход из ВНЕШНЕГО цикла). Единственная survey-находка (2026-07-02) genuinely-absent (`consider`, medium). Форма — identifier (НЕ `'outer` — лексич. конфликт с char-лит; НЕ `:`-форма). Value-carrying `break outer x` — вне scope. Ниша — grid/matrix-сканы. | [spec/open-questions Q-labeled-loops](../../spec/open-questions.md) | P3 |
| `[M-nested-or-patterns]` | `\|`-альтернативы ВНУТРИ варианта: `Some(1\|2\|3)` / `(0\|1, y)` (top-level `\|` уже есть). Обобщение: hoist `\|`-сбор из parse_match в parse_pattern, тот же `Pattern::Or`. Range-in-arm НЕ добавляем (покрыт guard). `consider`, low (survey 2026-07-02). | [spec/open-questions Q-nested-or-patterns](../../spec/open-questions.md) | P3 |
| `[M-extensible-sum-types]` | `#extensible` на экспортированном sum: тип может расти без breaking change — через границу пакета match обязан иметь `_` (`E_MATCH_EXTENSIBLE_NEEDS_WILDCARD`); внутри пакета exhaustiveness полная. Прецедент боли: D302 (NetError +2 варианта = breaking, прощено pre-release окном). Авто-`_ => panic` отвергнут (compile-гарантия → runtime-краш). **Gated на стабилизацию std / registry (Plan 03.3)** — до внешних потребителей не даёт ничего. | [spec/open-questions Q-extensible-sum-types](../../spec/open-questions.md) | P3 |

## By-design / WON'T-DO (не actionable — кандидаты в dead-markers)

| Маркер | Почему не делаем |
|---|---|
| `[M-118-aliasing-xor-rules]` | Rust-style XOR aliasing намеренно НЕ нужен (GC + auto-promote); revisit только если перф потребует. |
| `[M-118-inline-assembly]` | Inline asm — вне scope языка. Открыт лишь в тривиальном «не реализовано». → drop. |
| `[M-118-lifetimes-rust-style]` | Rust lifetimes — вне scope (Nova GC + Go-style auto-promote). → drop. |
| `[M-134-stdlib-ptr-alias]` | Опциональный prelude-алиас `type ptr = *()` (если `*()` читается шумно). Пользовательское решение — НЕ ставим в prelude по умолчанию (явный `*()` самодокументируется). Home: Plan 134 Followup. |

---

## Planned (НЕ floating — указатель)
| Маркер | План-дом |
|---|---|
| **Plan 147 — ✅ CLOSED 2026-06-12** | 3-axis mutability D246 (L1 binding / L2 view / L3 pointee) landed Ф.1-Ф.6 на plan-138.1 (НЕ смёржен в main). D245 flip-scan RETRACTED, `*T≡*ro T` восстановлен универсально. parser+checker+codegen + миграция codebase + oracle 30/0. Закрыл `[M-138-binding-type-mut-conflict]` (P6 split) + `[M-138.5-right-binding-migration]`; разгеёчил `[M-139-f0-lang-item-decl]`. Маркеры ниже — документированные границы (P2/P3), не блокеры. |
| `[M-147-infer-call-ret-mut-axis]` (P2) | Plan 147 Ф.3 — см. полное описание выше (call-return inference только ro/pointer-shaped + all-overloads-agree). Soundness через C-уровневый const-pointee. |
| **Plan 140 — ✅ CLOSED Ф.0-Ф.5 2026-06-12** (branch `plan-140`, НЕ merged) | Contracts enforced in release (enforce-with-elision). Закрыта safety-дыра: `requires`/`ensures` (D24) стирались в release **независимо от доказанности** → silent UB в недоказанных местах. Теперь: Z3-proven → элидируется (zero-cost), unproven → runtime-check и в release (fail-fast abort), `#unchecked`/`--contracts=off` — явный opt-out. D24 amend + Q34. 7/7 plan140 + 295/0 contracts release; F1-F7 met; perf: proven-elide zero-cost, unproven ≈+12% на contract-saturated loop. Маркеры ниже — deferred/ready, не блокеры. |
| `[M-140-bounds-as-contract]` ✅ **CLOSED 2026-06-13** ([Plan 140.2](140.2-vec-bounds-as-contract.md), branch `plan-140.2-bounds-contract`) | Prerequisite-first. **Part A (D256):** `@field`/accessor-`@method()` self-access в SMT-контрактах — encoder `@`→`_self` (через существующий `_field_<name>(obj)` UF; backend не трогается); checker reject non-accessor `@method()`; снят блокер E2401 (теперь `requires 0<=i && i<@len` кодируется). **Part B (D257):** Vec `@index`/`mut @index` несут `requires 0<=i && i<@len` (ручной `if…panic` СОХРАНЁН как runtime-guard для прямых `.index()` — generic-mono contract-enforcement пока не эмитится, Plan 140 gap; чинит др. агент); codegen context-sensitive — inline Vec-read переведён на lvalue-safe `(*({…;&data[i];}))` (`v[i].field=x`/`&v[i]`/`v[i].mut_method()` не сломаны). **B.4 элизия:** `prove_vec_index_sites` (verify, гейт non-trivial backend, pre-scan loop-less fn) доказывает `0<=idx<v.len()` per READ-сайт под loop-bound + **sound frame-check** (read-only цикл ⇒ длина инвариантна) → proven `v[i]` элидируется (спот-чек C: нет `nv_panic_index_oob`), недоказанный → runtime abort (debug И release). `@get`/`@first`/`@last` — None на OOB, без контракта. Тесты (z3): contracts 297/0, basics + vec-heavy (plan138*/139*/128/131/90_1) 0 новых FAIL, plan140_2 4/0 + 2 contracts (elision-pos/soundness-neg/lvalue/direct-call/@field-pos/@method-neg). Коммиты 9a667fbc(A)/e8e8ab38(B core)/5a4c366e(B.4). Остаток — `[M-140.2-elision-writeback]`. См. `[M-opt-elide-proven-overflow-checks]` (sibling, не-контрактный путь). |
| `[M-140.2-elision-writeback]` ✅ **CLOSED 2026-06-13** (§1–§3) | **§1 write-back** ✅ `for i in 0..v.len() { v[i]=f(v[i]) }` — frame-check len-инвариантный (`v[i]`-read + in-place `v[i]=val`-write + аксессоры; length-changing/`&v`/передача/реассайн → НЕ len-safe); элидирует запись И чтение (`b4_writeback_*`). **§2 slice** ✅ `v[a..b]` — verifier доказывает `0<=a && a<=b && b<=v.len()`; inline slice-проверка элидируется; fn-level frame-safety даёт `v[0..v.len()]` вне цикла (`followup_slice_elision_pos`). **§3 cross-fn** ✅ `v[i]` внутри `fn helper(v,i) requires 0<=i && i<v.len()` элидируется (bound из requires) — **2 proven-множества**: always-safe (loop/code, элидится всегда) vs contract-based (requires-зависимые, codegen элидит ТОЛЬКО при включённых контрактах; под `--contracts=off`/`#unchecked` проверка остаётся — `followup_crossfn_*`, спот-чек C). Verifier различает двойным доказательством (без/с requires). plan140_2 9/0; 0 новых регрессий. **Остаток (отд. фичи, не блокеры):** caller-side requires-CHECK элизия (per-method all-or-nothing, low-value); декларативный `requires` на `@index(Range)` (slice safety уже через inline-gate). См. `[M-opt-elide-proven-overflow-checks]` (sibling). |
| `[M-140.2-contract-exprdisplay-selfaccess]` ✅ **CLOSED 2026-06-13** | Diagnostic follow-up к D256: codegen-рендерер `expr_to_display` (emit_c) имел catch-all `_ => "assert"` и не покрывал `Member`/`SelfAccess`/`Index` → contract-violation `requires 0<=i && i<@len` печатался как `… i < assert`. Баг невидим тестам (custom-message маркер матчил только её; garbled `<src>` — в скобках `failed: msg (<src>)`). Всплыл, т.к. Part A впервые разрешил `@`-аксессоры в контрактах (generic-mono фикс лишь сделал видимым в mono'd телах). Фикс: армы для Member(`@field`/`obj.field`)/SelfAccess(`@`)/Index/char-float-lit/Path/`as`/`is`/turbofish + дефолт `"assert"`→`"<expr>"`; `typeref_display` → `pub(crate)`. Семантика проверки не тронута. Тесты: `plan140_2/contract_exprdisplay_selfaccess_{pos,neg}` (neg EXPECT `requires failed: @count > 0 && @count <= @limit`); plan140_2 14/0; адверсариальный C-инспект подтвердил литерал `@count…`/`@len` (не «assert»). Спека D256 §«Диагностика». |
| `[M-140-contract-panic-unwind]` ✅ **CLOSED 2026-06-13** (Plan 140.3, `60e909a0`) | Переосмыслено: не «abort→unwind» (assert/контракт в файбере уже разматываются к fail-frame, не abort), а **унификация классификации** — `nova_assert_loc`/`nova_contract_violation` теперь тегают fail-frame `error_kind = NOVA_THROW_PANIC` (как `nv_panic`, D188) → пойманный consume/supervised assert/контракт классифицируется как **Panic** (не Failure), по spec D13 «assert = panic». 2 строки в хедерах, без ABI/codegen. Тест `plan140/consume_assert_contract_panic_class`; 0 регрессий (plan110 50/0, plan100_4 42/0, plan125_1 15/0, contracts 251/0). |
| `[M-140-contract-levels]` ✅ **CLOSED 2026-06-14** (Plan 140.3, `77932ea3`) | Q34 §3. **(a) module-opt-out:** `#unchecked` перед `module X` (голый, консистентно с `#stable`/`#no_prelude`/`#forbid`, НЕ `#unchecked_module`). **(b) Eiffel per-kind:** `#unchecked(requires)`/`#unchecked(ensures)`/`#unchecked(invariant)` (комбинируемо) на fn И module уровне. AST `ContractOptOut{requires,ensures,invariant}` (заменил `bool`) + `Module.contract_opt_out` + `ModuleAttrKind::Unchecked`; codegen gate `contracts_elided_for(kind)`/`invariants_elided_here()` (requires/ensures/decreases/invariant независимо). Тесты `plan140_3/*` (neg-пара доказывает независимость: элизия requires → violation всплывает как `ensures failed`, и наоборот). 4/0; 0 регрессий (contracts 251/0, plan140 42/0, basics 8/0). **[Plan 194 A4 RETRACTED 2026-07-15]:** `#unchecked` (per-fn/per-module opt-out, все виды/kinds) физически снесён — парсер (`parse_unchecked_kinds`, `ContractAttrs.contract_opt_out`, `ModuleAttrKind::Unchecked`), AST (`ContractOptOut`, `Module.contract_opt_out`, `FnDecl.contract_opt_out`), codegen (`contract_opt_out_fn`/`contract_opt_out_module` поля; `contracts_elided_for`/`invariants_elided_here` теперь константно `false`) удалены целиком. Роль ушла в `#debug`-режимы (Plan 194 A2.1/A2.2) + Z3 sound-элизия. См. [D421](../../spec/decisions/09-tooling.md#d421-contract-execution-model--debug-dev-only-префикс--contracts-уровни-ретракция-uncheckeddebug_assert-plan-194-2026-07-14). |
| `[M-codegen-libtest-contract-opt-out]` ✅ **CLOSED 2026-06-14** (Plan 140.3 followup, merged `e83c8914`) | Plan 140.3 (`77932ea3`) добавил `Module.contract_opt_out: ContractOptOut`, но 4 тест-онли `Module{}`-инициализатора не обновили (`compiler-codegen/src/types/mod.rs` ×3: make_module/make_module_with_types/equal-synthesis + `protocols/auto_derive.rs` ×1: module_with) → `cargo test -p nova-codegen --lib` падал E0063 (×4), **блокируя ВЕСЬ nova-codegen lib-test таргет** (release-бинарник не затронут — чистый Rust, поэтому прод/`nova test` работали). Фикс: `contract_opt_out: Default::default()` (ContractOptOut деривит `Default` = полный enforce, семантически-нейтральное значение). **Критерии приёмки:** lib-test таргет компилируется (был E0063-сломан) ✓; test-only, ноль изменения поведения ✓; «без упрощений как для прода» — настоящий `Default`, не хак, конкретные правки (не закомментировано) ✓. Verified: `cargo test --lib --no-run` exit 0 → сьют запускается (**770 PASS**). Негативный кейс (до фикса): тот же `cargo test --lib` падал на compile E0063 ×4. |
| `[M-codegen-libtest-stale-tests]` ✅ **CLOSED 2026-06-14** | Починка компиляции lib-test ([M-codegen-libtest-contract-opt-out]) **вскрыла 33 pre-existing устаревших unit-теста**, которые НИКОГДА не запускались, пока таргет не компилировался (накопленный невидимый дрейф; **НЕ регрессия** фикса — фикс лишь сделал их runnable). Состав: **26 `parser::tests::*`** (closure_full_*/closure_light_*/trailing_*/fn_block_body/for_in_range/handler_lit_in_with/type_record) — тест-ИНПУТ использовал `let`, удалённый в **Plan 114/D184** → мигрировано `let`→`ro/mut` в тест-инпутах (Rust `let` в самом тест-коде сохранён, мигрирована только Nova-строка); **2 `lints::tests::prelude_shadow::*`** — мигрированы на `#no_prelude`/`#allow(shadow)` (Plan 107/D174 убрал inline-клаузы); **5 `codegen::sum_schema_registry::tests::*`** — обновлены ожидания routing `Option.unwrap_or` (Nova-body миграция Plan 99). **БЕЗ изменений прод-кода** — только тест-инпут+ожидания; «без упрощений как для прода» — ассерты остались осмысленными, ничего не замаскировано. Финал **803/0** (`cargo test --lib`, был 770/33). | floating (test-suite hygiene) | P2 |
| **Plan 140.1 — ✅ CLOSED Ф.0-Ф.3 2026-06-12** (branch `plan-140.1`, НЕ merged) | Contract & assert diagnostics: (A) единый короткий **location-first** формат `<file>:<line>: <kind> failed: <expr>` для requires/ensures/invariant И assert/debug_assert (RETRACT verbose `contract <kind> failed in <fn>: <expr> at <file>:<line>` + `assertion failed`); (B) опц. пользовательское сообщение на контрактах И assert (`requires x>0, "msg"` / `assert(c, "msg")`). LOC-префикс ВСЕГДА авто-добавляется codegen'ом (`__FILE__`/`__LINE__`-эквивалент из span). D24 AMEND (формат+сообщение) + D13 AMEND (assert формат+file:line). 9/9 plan140_1 + broad sweep 0 new FAIL (basics/contracts/plan140/plan139/str/plan147/generics). Закрыл `[M-140-contract-message]`. Маркер ниже — deferred, не блокер. |
| `[M-140.1-message-interpolation]` ✅ **CLOSED 2026-06-14** (Plan 140.3, `1d6d2ca5`+`83755d15`) | interp-сообщения контрактов `requires x>0, "got ${x}"` (синтаксис Nova `${...}`, не `{x}`). Переиспользует interp-машинерию (`desugar_string_interpolation`+`emit_interpolated_str`) → идентично обычной `"${x}"`-строке. `Contract.message_expr`; codegen `emit_contract_check` строит сообщение LAZY (только при провале, внутри `if(!cond){…}`) → `nova_contract_violation_dyn` (nova_str, `%.*s`). **Scope:** requires ✅ (mono+non-mono, `1d6d2ca5`); **ensures `${result}` + invariant `${field}` ✅ DONE (`83755d15`)** — `substitute_result_var_in_code` (byte-level, **string-literal-aware**, UTF-8-safe) переписывает `result`→`_nova_result` ТОЛЬКО в коде, не трогая контент `"..."`-литералов сообщения (иначе `"result was "` корраптился в `"_nova_result was "`); invariant-shadow-locals регистрируются в var_types на время эмиссии. `${примитив}` (`f32`/`f64` и т.п.) — через interp fast-path (прямой `@display(sb)`+typed `@append`), отдельный маркер Plan 154.1. Тесты `plan140_1/contract_msg_interp_{pos,neg}` + `ensures_msg_interp_{pos,neg}` + `invariant_msg_interp_{pos,neg}`; 0 регрессий (plan140_1 **15/0**, contracts 251/0). |
| **Plan 139 — ✅ CLOSED 2026-06-11** + **Plan 139.1 — ✅ CLOSED 2026-06-12** + **Plan 139.2 — ✅ CLOSED 2026-06-12** | umbrella str=value-record закрыт Ф.0-Ф.7; spec финализирован (D26 MAJOR AMEND + D216 §1 + D228 + D52); 0 new FAIL. **Plan 139.1 (E1/E4 completion):** корневой `[M-139-f0-lang-item-decl]` ✅ **ЗАКРЫТ И УДАЛЁН** — `type str value priv {ptr *u8,len int}` объявлен (`std/prelude/core.nv`), privacy fires, ABI-alias `nova_str`, 3 neg-фикстуры PASS; **БЕЗ новой checker-инфры** (переиспользован value-record Plan 124.8). E1 → ✅ FULL. E4 → ✅ FULL (закрыто Plan 139.2). **Plan 139.2 (full str-method Nova migration, ветка plan-139.2, НЕ merged):** закрывает E4-остаток — **9/10 str-методов мигрированы external-C → Nova-body** (Ф.0 @as_bytes, Ф.1 @len/@byte_at, Ф.2 @split/from_bytes×3, Ф.3 @concat/@compare); `@hash` остаётся C (Ф.4, SipHash+crypto-seed, security). Cross-type мост: `Vec[T].from_raw_parts`/`@as_ptr`/`consume @into_raw`. КЛЮЧЕВАЯ ПЕРЕОЦЕНКА: privacy у Nova **type-based**, не module-based → str-метод в string.nv видит priv `@ptr`/`@len` и конструирует `str{…}`. spec **D247** (08-runtime.md) umbrella. plan139_2 12/0, 0 регрессий. История — simplifications.md (Plan 139.2 Ф.0-Ф.5). Маркеры ниже — permanent/perf-only followups, не блокеры. |
| `[M-139-f0-rt-header-ptr-sign-casts]` | Plan 139 Ф.5 — 59 -Wpointer-sign warnings в рантайм-C-хедерах (array.h/conv.h/effects.h/nova_rt.h) после typedef→`const uint8_t*`; source-compatible, подавлены `-w`; cast string-литералов отложен (часть 354-site работы) |
| `[M-139-f1-trim-view]` ✅ **CLOSED Plan 152.2 Ф.2 (2026-06-13)** | `str @trim()` теперь ZERO-COPY — возвращает sub-view `@[start..end]` (codegen лоуэрит в `(nova_str){.ptr=…+start,.len}` напрямую) вместо alloc-копии. Заодно `@trim_start`/`@trim_end` (тоже zero-copy). НЕ потребовал прямой `str{ptr:@ptr+off}` конструкции (она мис-компилируется, `[M-152.1-str-subview-record-ctor]`) — `@[a..b]` обходит. str иммутабелен (R8) + GC держит буфер живым через view. История — simplifications.md (152.2 Ф.2). |
| `[M-139.1-hash-irreducible-crypto-seed]` — 🟢 **CONFIRMED-BY-FACT Plan 139.2 Ф.4 (2026-06-12)** | Plan 139.1 Ф.B → **подтверждён фактом из исходника** (Ф.4): `str @hash()` = **SipHash-1-3 + per-process random crypto seed** (`nova_siphash13` + `nova_hash_seed_k0/k1`, init через `getrandom`/`BCryptGenRandom`, nova_rt.h:265-343) — **НЕ FNV-1a** (устаревший комментарий string.nv исправлен в Ф.4). Это **ПОСТОЯННАЯ обоснованная C-граница (security)**, НЕ TODO/НЕ followup-к-миграции: DoS-resistance (hash-flooding защита) ТРЕБУЕТ чтобы seed оставался Nova-невидимым — экспонировать seed на Nova-сторону = security/DoS-регрессия HashMap. Остаётся `external fn` (C `nova_str_hash`). 9/10 str-методов мигрированы в Nova-body (Ф.0–Ф.3); `@hash` — единственное обоснованное C-исключение. Маркер закрыт как permanent-boundary (записан для аудита, не требует дальнейших действий). `[M-139-f2-ptr-field-producers]` — ✅ ЗАКРЫТ Plan 139.2 Ф.0+Ф.2, строка убрана (история — simplifications.md + D247) |
| `[M-139.1-operator-lowered-methods]` — ✅ **CLOSED Plan 152.5a D-R4 (2026-06-13)** | **Декомиссия выполнена (override option (b)).** Операторы `==`/`!=`/`<`/`<=`/`>`/`>=`/`+` для str теперь синтезируются из Nova-body (`emit_c.rs` nova_str BinOp arm → `Nova_str_method_eq` / `Nova_str_method_compare(l,r) OP 0` / `Nova_str_method_concat`). Реестр str (`runtime_registry.rs` + `str_method_to_rt`) = **только `@hash`**. Perf-паритет: `@eq` length-first+`RawMem.compare`(memcmp), `@compare` RawMem.compare+tiebreak, `@concat` with_capacity+2×Vec.append(memcpy)+steal — те же примитивы что снятые C-fn; бенч 5M cmp+200k concat: before 23.5–28.2s ≈ after 22.6–29.2s (дельта в compile-шуме). C `nova_str_eq`/`concat` остались ТОЛЬКО как codegen-внутренние примитивы (структурная eq полей в `emit_field_eq` HashMap-ключей/record-`==`; ScopeOutcome cancel-marker + concat-acc) — не user-facing операторы. 0 регрессий + reachability-проба чистых операторов PASS. Док: [03-syntax.md D254 D-R4 DONE]. ~~Исходный REFRAMED-текст (option (b), 139.2 Ф.3):~~ **Решение зафиксировано: option (b).** `str @concat()`/`@compare()` МИГРИРОВАНЫ в Nova-body (139.2 Ф.3): прямые method-вызовы (`s.concat(t)`/`s.compare(t)`, Compare-протокол `@compare(o)==0`, `@plus`-body, `@replace`-chain) идут в Nova-body (убраны из `str_method_to_rt`, emit_c.rs). ОПЕРАТОРЫ `+`/`<`/`<=`/`>`/`>=`/`==`/`!=` СОЗНАТЕЛЬНО ОСТАВЛЕНЫ на прямом C-lowering (`nova_str_concat`/`nova_str_lt`/…/`nova_str_eq`, BinOp-arm `lty=="nova_str"`) — **option (b)**, НЕ (a). **Почему:** (1) **perf** — operator-формы горячие (string building, sort): C = один alloc+2×memcpy / один memcmp; Nova-body = with_capacity+2 byte-push-loop'а / byte-loop с per-byte `as int`. (2) **ортогональность** — operator-lowering (BinOp codegen) и method-dispatch — независимые механизмы. Дубль (Nova-body метод + малый inline C-fn для оператора) приемлем. **Остаточный scope маркера:** будущее ЧИСТОЕ retirement C-fn для операторов = СОВМЕСТНАЯ миграция operator-lowering→Nova-method-dispatch + perf-харнесс (подтвердить нет регрессии). Orthogonal, низкий приоритет. `@eq`/`@lt`/`@le`/`@gt`/`@ge` остаются C-method-routed (не мигрированы — операторы их единственный горячий путь). Home: будущий str-operator-Nova-body план. Док: [02-types.md «Amend (Plan 139.2 Ф.3)»], [08-runtime.md D26 «Методы — Nova-body»] |
| `[M-139.1-len-d117-method-only]` — 🔵 **RESOLVED/REFRAMED Plan 139.2 Ф.1 (2026-06-12)** | Plan 139.1 Ф.B исходно постулировал «`str @len()` НЕ мигрируем — `len` под D117-баном `E_SIZE_ACCESSOR_FIELD`, остаётся external `nova_str_byte_len`». **Plan 139.2 Ф.1 пересмотрел и МИГРИРОВАЛ `@len` в Nova-body** через D117 **self-field carve-out**: bare `@len` field-read внутри declaring type-метода str (obj=`SelfAccess` ∧ `current_receiver_type==Some("str")` ∧ `name=="len"` — реальное поле value-record'а) ИЗЪЯТ из D117-бана (emit_c.rs ~17620). `str @len() => @len` (O(1) bare priv-field read). **ВНЕШНИЙ `s.len` по-прежнему HARD ERROR `E_SIZE_ACCESSOR_FIELD`** — carve-out не ослабил enforcement (D117 external-negatives plan60/f3 + plan139/neg_t0 PASS как negatives). Документировано: D117 amend (03-syntax.md «self-field carve-out для declaring type-method»). Маркер — RESOLVED, записан для аудита (объясняет почему `@len` теперь Nova-body, не external) |
| `[M-139-f4-to-cstr-owning]` | Plan 139 Ф.4 followup → совпадает с `[M-118.1-cstr-to-cstr-distinct-copy]` (Plan 118.2): owning `@to_cstr()` (буфер, переживающий source str — needs malloc/free API). Ф.4 НЕ deferred — D26 §3 `as_cstr` alloc-fallback РЕАЛИЗОВАН (NEW C-примитив `nova_fn_nova_str_terminated_ptr`: peek `ptr[len]` + conditional `nova_alloc`). Этот примитив = естественный home для будущей owning-copy (alloc-ветка уже делает GC-tracked копию; owning-вариант снимет zero-copy fast-path). eq/hash/clone (doc-Ф.3) — НЕ in scope этой задачи: str ==/< уже content-eq через direct BinOp lowering (emit_c.rs:16985), не Plan 141 field-by-field |
| `[M-139-f3-bare-return-type-str]` | Plan 139 Ф.3 — pre-existing compiler-баг (НЕ введён Ф.3): top-level `fn f(x str) str` (bare return-type, БЕЗ `->`) лоуэрит return-тип как `nova_unit` → CC-FAIL «returning 'nova_str' from a function with incompatible result type 'nova_unit'». Канонический `-> str` работает корректно. Парсер bare-return-type формы теряет/мисс-парсит return-тип. Низкий приоритет (one-obvious-way = `->`); вынесено при написании t3_clone_independent fixture |
| `[M-139-f6-vec-mut-local-enforcement]` | Plan 139 Ф.6 — DISCOVERED (НЕ введён Ф.6; pre-existing на plan-138.1, вне зоны literal-lowering diff): plan108_2 neg-фикстуры `let_nomut_{array_push,clear,pop,truncate}_neg` ожидают `E_LOCAL_NOT_MUT` при вызове Vec-мутирующих методов (push/clear/pop/truncate) на `let` (non-mut) локале, но codegen succeeds (NEG-NO-ERROR, 9/4). Gap из 138.x Vec-миграции: mut-enforcement на Vec-методах не срабатывает для local-binding mutability (D36). Home: Plan 138.x Vec-mut follow-up. Низкий приоритет (orthogonal к str) |
| `[M-139-interning]` (НЕ ОТКРЫТ — landed in full) | Plan 139 Ф.6 — doc допускал defer dedup-interning если «risky/large». Реализация small (1 файл emit_c.rs, +113/-6) + low-risk (R14/R15 LOW, semantically invisible) → landed целиком, маркер НЕ открыт. Per-CU rodata dedup идентичных литералов: один `static const uint8_t[]` + `static const nova_str` на distinct content; FNV-1a content-hash символы. Записан здесь для аудита (что defer-опция рассмотрена и отклонена в пользу полной реализации) |
| `[M-147-infer-call-ret-mut-axis]` (P2) | Plan 147 Ф.3 — checker `infer_expr_type` пропагирует return-тип для coercion/deref-write gate ТОЛЬКО для `ro`-wrapped и pointer-shaped returns, и только когда ВСЕ overload'ы согласны (call-resolution не выполнена на этом этапе). Method-call return (`v.f()`), generic-return, mixed-overload → `None`→no-gate. L3/coercion-нарушение для таких форм ловится позже C-компилятором (`const T*` write = CC-FAIL), не чистой Nova-диагностикой. Home: Plan 147 follow-up (полноценный return-type inference + monomorphization в checker'е). Soundness сохранён (отвергается, но позже) |
| `[M-147-deref-write-compound-lvalue]` (P2) | Plan 147 Ф.3 — L3 deref-write gate (`*p=v`→E_POINTER_RO_ASSIGN) срабатывает когда `p` — Ident/As-cast с известным типом в scope. Составные lvalue (`*(p+i)=v` Binary operand, и пр.) дают `infer_expr_type=None`→no-gate; ловится C-уровнем (const-pointee). Home: Plan 147 follow-up. Soundness сохранён |
| `[M-147-generic-element-deref-write]` (P2) | Plan 147 Ф.3 — oracle row E `Vec[*T]` vs `Vec[*mut T]` (`*v[i]=x`): element-deref-write через generic-instance index НЕ enforced на Nova-уровне (требует element-type inference через `[]` на mono'd generic в checker'е). Документирован в oracle (02-types.md D246), ловится C-уровнем (const element pointee). Home: Plan 147 follow-up. Soundness сохранён |
| `[M-147-null-star-ptr-retraction-guard]` (P3) | **PRE-EXISTING, обнаружен Plan 147 Ф.4 (не регрессия Ф.4).** Парсер `parse_primary` emit'ит `E_NULL_PTR_RETRACTED_USE_OPTION` только для `null <bare-prim-ident>` (`ptr`/`int`/…/`str`); форма `null *()` (typed-pointer-literal, Plan 134) не покрыта → fall-through → `undefined identifier null`. Фикстура plan118/t5_neg_null_ptr_retracted = NEG-WRONG-MSG. Регрессия с Plan 134 commit c41d568ae2c (тело `null ptr`→`null *()`, guard не расширён). Fix: расширить guard на `null` + Star/Pointer-type position. Orthogonal к 3-axis. Home: parser cleanup. Hard-error сохранён (другой код) |
| **Plan 179 — 🟢 Ф.1 (decode) + Ф.3 (encode) LANDED 2026-07-04** (branch `plan-179-encoding-compress`) | `std/encoding/compress` pure-Nova: inflate/gzip/zlib **decode** (Ф.1) + deflate/gzip/zlib **encode** (Ф.3: `CompressLevel` levels, LZ77+fixed/dynamic-Huffman, streaming `Deflater`/`GzipWriter`/`ZlibWriter` + SYNC-FLUSH). Round-trip `inflate(deflate(x))==x` все уровни; **external oracle** (python zlib/gzip + `gzip -d`) подтвердил RFC 1950/1951/1952. conformance 38/0. **UNBLOCKS Plan 178 Q12 (gzip/deflate) СЕЙЧАС.** Маркеры ниже — build-gate + followups. |
| `[M-179-brotli-vendor-lib]` (P2, build-gate) | **Plan 179 Ф.2 (brotli decode C-FFI) заблокирован: vendor-lib `libbrotlidec` отсутствует эмпирически (2026-07-04).** grep worktree=0; main-repo vcpkg (gate-env target) = только gc/z3/atomic_ops/cord — brotli.lib/headers нет; vendored native = только `target/libuv-cache/libuv.lib`. Ф.2 стартует ТОЛЬКО после vendor-коммита google/brotli (BSD-2/MIT, статика как libz3/libuv, §6). C-FFI без либы НЕ фейкается (§0/§7.7). Home: Plan 179 Ф.2 / §11. |
| `[M-179-std-compress]` (умбрелла) | Plan 179 followups (§11): pure-Nova-brotli (убрать C-FFI), brotli-encode, zstd/lz4/lzma, zlib preset-dictionary (`Inflater.new_with_dict`), optimal-parse level-9, cross-block streaming-encode-matches (V1 = self-contained на flush), comptime codec-tables, `Crc32`/`Adler32` value-обёртки, `copy_to`/streaming-to-file, gzip-single-member. Home: Plan 179 §11. |

## Follow-up: stale-tag cleanup
Триаж (w33ant6rp) нашёл **34 маркера с устаревшим OPEN-тегом** (30 RESOLVED + 4 SUPERSEDED — gap закрыт, текст висит): `[M-115-ptr-arithmetic]`, `[M-83.10.4-residual-flaky]`, `[M-83.10.4-supervised-cancel-armed-race]`, `[M-138-getmut-rename]` (superseded) + 30 resolved (полный список в workflow-output w33ant6rp). **Followup:** поправить их статус в source-планах (отдельный doc-проход), чтобы grep по OPEN был честным.

## Follow-up: Plan 152.0/152.1 (str-модуль) — остаточные маркеры
- **`[M-152.1-str-subview-record-ctor]`** (planned, **home Plan 152.2**, P2): конструкция
  str sub-view через value-record-литерал с pointer-арифметикой на priv `@ptr` —
  `str { ptr: @ptr + off, len: n }` в str-методе — **мис-компилируется** («passing
  nova_str to nova_int»); т.к. `runtime.string` линкуется в КАЖДУЮ программу, это
  ломает весь str-код. Обнаружено в Plan 152.1 Ф.1b при попытке сделать `str
  @index(Range)`/`@get(Range)` через прямую конструкцию. **Обход (приземлён):** оба
  метода строят view через inline `@[a..b]` (codegen лоуэрит в `(nova_str){.ptr=…+from}`
  напрямую, минуя Nova-конструкцию value-record'а). Класс — value-record codegen (как
  закрытые в Ф.3 self-return/dispatch, но для record-literal-construction с @ptr-arith).
  **Home Plan 152.2** (full str-surface — прямой потребитель: zero-copy `trim`/`strip`/
  slice-хелперы строят такие суб-вью; обход `@[a..b]` ограничивает). Fix чинит И парный
  **`[M-139-f1-trim-view]`** (zero-copy trim вместо alloc-копии — та же потребность).
- **`[M-raw-ptr-local-index-codegen]`** (floating, P3): индексация raw-pointer-**локала**
  не кодгенится — `ro p = @ptr; p[i]` → C «subscripted value is not a pointer», хотя
  `@ptr[i]` напрямую (field) работает (`@byte_at`). Обнаружено в Plan 152.1 (RawMem.compare
  обходит — передаём `@ptr` в fn). Fix если понадобится raw-ptr-local-index: emit_c трактует
  bound `*ro u8`-локал как скаляр, не указатель.
- **`[M-152.1-str-index-range-contract]`** ✅ **ДОСТИГНУТ (цель) Plan 152.1 Ф.1b 2026-06-13.**
  `str[a..b]` теперь **byte-range** zero-copy slice с **элидируемой** bounds-проверкой
  (140.2-style `index_site_elided`, zero-cost когда провабельно) + UTF-8 codepoint-boundary
  guard — inline в codegen (зеркало Vec[T] elidable-slice), `nova_str_slice_panic` больше не
  диспатчится. Заодно semantics codepoint→byte (чинит non-ASCII split + pre-existing split_edge).
  **Остаток (deferred, не блокер):** отдельный Nova `str @index(Range)` метод (как `vec/slice.nv`,
  Index[Range,str] protocol-formalism + routing) — отложен консистентно с Vec's собственной
  Range-routing migration (`v[a..b]` тоже пока codegen-inline, не Nova-метод).
- **D117 на `prefix.len`** (не баг — by-design до Plan 153): чтение size-accessor `len` как
  поля чужого инстанса → `E_SIZE_ACCESSOR_FIELD`. Internal-field-read **D117 AMEND у Plan 153**
  (Ф.5.1). До него str-методы используют `other.len()` (метод), не `other.len` (поле).
- **`[M-139.1-operator-lowered-methods]`** (planned, home **Plan 152.5a / D-R4**): декомиссия
  хардкода str-операторов в `emit_c.rs:17302` (`<`/`==`/`+` → C `nova_str_lt`/`eq`/`concat`) —
  синтезировать из `@compare`/`@eq`/`@concat`; после — удалить реестровые `eq`/`lt`/`le`/`gt`/`ge`
  (реестр str → только `@hash`). Perf через RawMem в Nova-body (без perf-retain C, override автора).
- **Урок (методология baseline)** — [152-gate-verification.md](152-gate-verification.md): не убивать
  baseline досрочно (частичный → ложные «регрессы» в непокрытом хвосте); экстракция имён через
  `awk '$1~/FAIL/{print $2}'`, не regex по строке; main-бинарь = быстрый оракул «новое vs pre-existing».

## Follow-up: Plan 154.1 (#impl-конформность + Display/Debug примитивов)
- **`[M-154.1-box-generic-static-ctor]`** ✅ **RESOLVED 2026-06-14** (by plan-153.1): the
  `(Box)[T];` raw-C leak from the 154.1 probe is no longer reproducible — plan-153.1's
  generic-static/method codegen fixes (`f7f56f0f` overload-mono, `3d9a7361` variadic,
  `8d493e5a` value-record slice) landed after the marker was filed and cover it. Verified
  5 construction forms (concrete `Box[int].new`, generic-context `wrap`, nested
  `Box[Box[int]]`, `.of` overloads in plan153_1/generic_overload). Regression guard
  `plan154_1/pos_box_generic_static_ctor`.
- **`[M-154.1-static-call-unresolved-loud]`** ✅ **RESOLVED 2026-06-14**: `Prim.method(...)`
  на примитиве, дошедший до fall-through emit_c.rs ~24376 (все валидные primitive
  static-методы/интринсики резолвятся раньше) → `E_UNKNOWN_STATIC_METHOD` compile-error
  вместо undefined-символа `nova_fn_<prim>_<method>` на линковке. Узко: только примитив-
  ресиверы (модуль-qualified free-fn и user-типы не задеты). neg-тест
  `plan154_1/neg_unknown_static_method`; broad regression 0 новых FAIL. **Остаток (не-primitive
  путь):** общий случай «unknown free-fn в произвольном модуле» сложнее (fall-through
  обслуживает legit runtime-builtin + bootstrap-без-peer_files D134) — не покрыт, низкий приоритет.
- **`[M-154.1-required-conformance]`** → перенесён в **[Q37](../../spec/open-questions.md#q37-конформность-протоколов-opt-in-структурная-vs-required-номинальная--частично-2026-06-13-plan-1541--d268)** (открытый вопрос дизайна, не actionable-работа): opt-in (структурная) vs required (номинальная) конформность.
- **`[M-154.1-f32-display-debug]`** ✅ **RESOLVED 2026-06-14**: f32 получил `#impl(Display)`/`#impl(Debug)`.
  conv.h `nova_f32_to_str`/`_to_debug_str` (widen→double + f64-форматтер) + `@append(f32)`
  (`x as f64`) + ветка в interp-map. Заодно починен общий codegen-баг: self-call `@m(args)`
  overload-резолв по типам аргументов (был только по `recv_mutable` → `@append(x as f64)`
  брал базовый str-overload). plan154_1 6/6.
- **`[M-154.1-f32-literal-coercion]`** ✅ **RESOLVED 2026-06-14** (commit `4e0d340a`): приведение
  числовых array-литералов к f32 из контекста — приведение кодогена в соответствие с уже
  задокументированным правилом **[D44](../../spec/decisions/03-syntax.md#d44-числовые-литералы)**
  (контекст переопределяет default-тип). `try_emit_typed_vec_literal` берёт float-hint когда ВСЕ
  элементы — числовые литералы (FloatLit/IntLit); turbofish-static арг-array-литерал эмитится с
  param-C-типом (узко: только array-литералы, иначе ломались Result-арги sort). Работает:
  `Vec[f32].from([1.5,2.5])` / `from([1,2])` / `of(1.5,2)`. **Прод-граница (commit `bcf01137`):**
  f64-ПЕРЕМЕННАЯ в `[]f32`-контексте (`Vec[f32].from([f64var])`) — НЕ сужается молча (был тихий
  мусор), а даёт громкую `E_ARRAY_ELEM_NARROW` (D44/D54 — narrowing не-литералов только через
  явный `as f32`). pos+neg тесты `plan154_1` (9/9).
- **`[M-numeric-try-narrowing]`** ✅ **RESOLVED 2026-07-20** (worktree `nova-tryfrom`,
  ветка `p-try-narrowing`, не влито в main — интегратору на слияние). Проверяемое сужение
  число→число: `(300u32).try_to_u8() -> Result[u8, RangeError]` — метод на исходном значении
  (чейн-форма, НЕ Rust-static `u8::try_from`), `try_`+`to_<T>` naming ряд с `str.to_i8/i16`/
  `checked_*`. **Реализация — 10 `.nv`-бланкетов** (`fn[S Ints] S @try_to_i8()`/…/`@try_to_uint()`,
  один на целевой тип, `std/prelude/protocols.nv`) вместо N²=100 ручных методов: единый
  `Ints`-бланкет с sign-agnostic `if @ < 0 {} else {}`-веткой (форма `@saturating_pow`) покрывает
  ОБА знака источника одним телом (не split SignedInts/UnsignedInts на одно имя — нет прецедента
  двух одноимённых бланкетов над непересекающимися type-set в файле); каждое тело расширяет `@`
  до full-width `i64`/`u64` ПЕРЕД сравнением с границами цели (soundness — избегает wraparound
  при касте границы цели ВНИЗ в узкий источник, напр. `u32.MAX as i32 == -1`). Полная матрица
  10×10 (100 пар, вкл. identity + same-signedness widening — тривиально `Ok`): Nova type-sets не
  умеют width-based исключение, ручное выкусывание «безопасных» пар означало бы возврат к N²;
  тот же выбор, что у Rust `TryFrom` numeric impls. `RangeError` — новый unit-тип
  (`std/prelude/errors.nv`, re-exported facade, `PRELUDE_VERSION` 18→19) — НЕ переиспользован
  `ParseIntError` (str-parse-специфичные `Empty`/`InvalidDigit`/`InvalidRadix` нерелевантны число→
  число). Компилятор НЕ тронут (`types/mod.rs`/`emit_c.rs`/`lints.rs` — заняты параллельной
  Duration-волной). Тесты `std/src/math/try_narrowing_test.nv` (не рядом с `protocols.nv` —
  auto-import-prelude-disabled gap, тот же что у `overflow_policy_test.nv`); краевые: границы
  MIN/MAX точно → `Ok`, MAX+1/MIN-1 → `Err`, 0, отрицательное→unsigned → `Err`, Vec[T].of-сэмплы.
  Известный НЕ-новый разрыв (не заводился отдельным followup — покрыт существующим классом):
  `for`-bound receiver для generic type-set-bound бланкета падает в pre-existing `[P67-LEGACY]`
  "method call return type unknown" — обходится типизированной `ro`-локалью (тот же обход, что
  весь `overflow_policy_test.nv`). Спека — **D430** (`spec/decisions/04-effects.md` + README).
  Гейты (worktree-бинарь): `nova test std/src/math` 4/4 PASS; `nova lint --deny std` 5 находок —
  ВСЕ pre-existing в нетронутых файлах (`fmt_buf.nv`/`string_builder.nv`/`write_buffer.nv`,
  известный `[M-p200-17-remaining-3]`), 0 новых; флагман `examples/flagship/aggregator`
  `--strict-effects` built OK; 2 nova_tests-провала (`folder_per_file_imports_use`,
  `plan62/neg/prelude_shadow_warning`) подтверждены pre-existing идентичным прогоном на HEAD
  без моих правок (baseline diff).
- **`[M-154.1-chained-vec-f32-method-misdispatch]`** ✅ **RESOLVED 2026-06-14** (Plan 153.x):
  chained `Vec[f32].new().debug(a)` / `.from([...]).debug(a)` мис-диспатчил `.debug` на
  `str.debug` → `Vec[f32]*` в str-метод → CC-FAIL. **Корень — gap C (registration timing)**, НЕ
  infer turbofish-return (тот УЖЕ давал `Nova_Vec____nova_f32*` корректно): outer-`.debug`
  dispatch (block 5b ~emit_c.rs:23816) достигался ДО эмита inner `Vec[f32].new()`, поэтому
  инстанс `Nova_Vec____nova_f32` ещё не был в `generic_type_instance_info` → block 5b
  промахивался, fall-through на str `@debug` overload (Ф.3). `Vec[int]` работал лишь т.к.
  регистрируется prelude/std повсеместно. **Фикс:** зеркалирование emit-side static-call
  registration (~22724) на inference-пути (TurboFish-Member branch, ~32990) —
  `infer_expr_c_type(obj)` (вызывается dispatcher'ом ~22828 ДО block 5b) регистрирует инстанс +
  queue'ит worklist. Фикстура `plan153_3/vec_f32_chained_debug` PASS; 0 регрессий (plan154 5/5,
  plan154_1 9/9, plan153_3 8/8, basics 8/8, plan90/90_1/138/91 чисто).
- **`[M-float-roundtrip-printing]`** (floating, P3, выявлено в аудите 154.1): float→str
  (`nova_f64_to_str`, и f32 через него) использует `%g` с дефолтной 6-знач точностью — **лосси**
  на >6 значащих для f64 И f32 (не round-trip). Проектный стандарт, консистентный, НЕ регрессия;
  но для прод-качества stdlib желателен shortest-round-trip формат (Ryū/Grisu). Затрагивает
  весь stdlib-float — отдельная задача, не f32-специфична.
- **`[M-float-roundtrip-printing]`** (floating, P3, выявлено в аудите 154.1): float→str
  (`nova_f64_to_str`, и f32 через него) использует `%g` с дефолтной 6-знач точностью — **лосси**
  на >6 значащих для f64 И f32 (не round-trip). Проектный стандарт, консистентный, НЕ регрессия;
  но для прод-качества stdlib желателен shortest-round-trip формат (Ryū/Grisu). Затрагивает
  весь stdlib-float — отдельная задача, не f32-специфична.

## Follow-up: Plan 153.1 (Vec core API — отложенные из-за codegen-лимитов)
- **`[M-153.1-cap-setter-overload]`** ✅ **RESOLVED** (через фикс
  `[M-138.2-generic-method-overload-mono]` ниже): same-name `@cap(n)` write-setter (overload
  `@cap()` getter, D117 AMEND) полностью работает — statement + fluent chain
  (`v.cap(n).push(...)`). `@cap_to` переименован обратно в `@cap(n)`.
- **`[M-138.2-generic-method-overload-mono]`** ✅ **FIXED** (dispatch + chain return-infer):
  (1) DISPATCH — mono'd generic-type instance-method вызов различает overload'ы по
  арности+param-типам (call-site block 5b emit_c.rs ~22922 + `__<paramtype>` suffix ~22982,
  reuse erased-base mangle 2858; body-side side-map `mono_name→FnDecl`). (2) RETURN-TYPE
  inference для chained overloaded `-> @` setter (Ф.3 fallback ~32041 + `infer_mono_method_ret_with_args`
  ~29689 arity-disambig) — `v.cap(n).push(...)` инферит setter-возврат как mono-receiver, не
  getter-int. Box-arity repro + `@cap(n)` statement+chain PASS; STRICT no-op для 1-overload.
  **Разблокирует** (отдельные std-рефакторы): merge `@splice`→`@insert`-overload,
  append/extend-консолидация (`[M-153.1-append-extend-consolidation]`).
- **`[M-138.2-overload-no-match-typecheck]`** (planned, P2, home **Plan 138.2 / type-checker**):
  вызов generic-метод-overload'а **без совпадения** по арности/типам (напр. `b.slot(1, 2)` где
  `@slot` = 0-арг getter + 1-арг setter) сейчас **CC-FAIL'ит** на clang-стадии (codegen fall-
  through к первому кандидату → «too many arguments»), а не отвергается чисто type-checker'ом до
  codegen. Нужно: валидация overload-арности/типов на call-site (`types/mod.rs`, E_… +
  кандидаты-сигнатуры в hint). Surfaced при закрытии `[M-138.2-generic-method-overload-mono]`
  (dispatch для МАТЧАЩИХ overload'ов починен; no-match-диагностика — отдельный type-check слой).
  `EXPECT_COMPILE_ERROR` (Nova-codegen) сейчас не ловит этот кейс (codegen «успешен», падает clang).
- **`[M-153.1-append-extend-consolidation]`** (planned, home **Plan 153.1 / D259**): план
  хотел один `append` (concrete Vec bulk + generic Iter overload), `extend` убрать.
  Заблокировано тем же overload-collapse + у generic-`append` (`for x in items {@push(x)}`)
  self-append footgun (`v.append(v)` растёт во время итерации; bulk-версия снапшотит длину).
  Оставлены раздельно: `@append(Vec[T])` (bulk, self-safe) + `@extend[S Iter[T]]` (generic).
  Консолидировать, когда overload-mono + self-alias-safe generic append.
- **`[M-153-scalar-min-max]`** ✅ **CLOSED 2026-06-16.** `@min(other)`/`@max(other)` реализованы через Nova-body if/else (без C-макросов `max`/`min`) на всех 12 числовых типах в `std/runtime/defaults.nv`. Тест `plan153_1/scalar_min_max` PASS (release nova). Коммит `782a8e36`.
- **`[M-153.1-of-vs-from-sweep]`** (planned, P3, churn): конструктор-конвенция формализована
  (план 153.1 / D259 + док `from` в core.nv направляет на `of`): литералы → `Vec[T].of(a,b,c)`,
  конверсия коллекции → `from(coll)`. Опциональный sweep существующих `from([литерал])` → `of(...)`
  в тестах/stdlib — низкий приоритет (оба корректны; чистый churn в большом diff'е).

## Follow-up: Plan 153.6 (Vec-протоколы Hash + FromIterator)
- **`[M-153.6-vec-hashmap-key-eq]`** (planned, P2, home **Plan 153.6 / HashMap**): `Vec[T]`
  как ключ `HashMap`/член `HashSet` упирается в pre-existing HashMap-codegen-баг — collision-
  check `k.eq(key)` (`hashmap.nv:529`) НЕ диспатчит в Vec-`@equal` для generic-type ключа →
  CC-FAIL «no member named `eq` in Nova_Vec____nova_int». D237 переименовал `eq`→`equal`, но
  codegen-lookup `.eq()` для generic-типа не находит `@equal` (тот же generic-method-dispatch-gap
  класс, что overload `@cap`/`@splice`). `Vec[T Hash] @hash()` РАБОТАЕТ (153.6, plan153_6/hash
  3/3); это вторая (equality) половина ключ-контракта. Сурфейснуто 153.6.
- **`[M-153.6-fromiterator-gated]`** ✅ **RESOLVED** (2026-06-14, Plan 153.6 / D264):
  FromIterator / collect-target приземлён поверх ленивого слоя 153.2. Surface:
  `BoxIter[T] @collect()->Vec` (default) + `BoxIter[T Hash] @collect_set()->Set` (dedup,
  `vec_lazy.nv`) + композиция над собранным `Vec` для прочих таргетов
  (`Set.from_iter(it.collect())` / `HashMap.from(pairs.collect())`) + `Vec[T].new().extend(src)`
  (FromIterator из любого `Iter`-источника). plan153_6/collect_target 12/12. Остаток — два
  gated compiler-gap маркера ниже (НЕ упрощение).
- **`[M-153.6-collect-static-generic]`** (planned, P2, home **Plan 153.6 / codegen**):
  *статический* generic-конструктор `Vec[T].from_iter[S Iter[T]](src S)` с for-in по `S` в
  теле не компилируется — bound `S Iter[T]` не резолвится для for-in dispatch внутри static
  generic-метода (typevar остаётся `Nova_S`; CODEGEN-FAIL «for-in: type 'S' has neither…»).
  Тот же класс, что generic-method-dispatch-collapse. Рабочий обход — instance-`@extend`
  (`Vec[T].new().extend(src)`). NEG-фикстура `plan153_6/collect_static_generic_neg` лочит
  границу (`EXPECT_COMPILE_ERROR`); фикс → flip в позитив.
- **`[M-153.6-collect-map-tuple-receiver]`** (planned, P3, home **Plan 153.6 / parser**):
  прямой терминатор `BoxIter[(K, V)] mut @collect_map() -> HashMap[K, V]` не парсится —
  receiver type-аргумент кортежем (`BoxIter[(K, V)]`) отвергается («expected identifier,
  got `(`»). HashMap collect-target остаётся `HashMap.from(pipeline.collect())`. Фикс —
  принять composite type в receiver type-arg слоте парсера.

## Follow-up: Plan 153.2 (ленивый итератор — отложенные Phase B / perf)
- **`[M-153.2-generic-over-source-zerocost]`** (🟡 **PARTIAL — STAGE 1+2 реализованы**, P3,
  **perf-only — НЕ упрощение**, home **Plan 153.2 / [D277](../../spec/decisions/02-types.md#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2)**):
  **Stage 1 (by-value mono generic value-records, commit `0da18125`):** монорфизатор научен
  лоуэрить generic `value`-рекорд BY VALUE (inline `NovaValue_<short>`, без `nova_alloc`
  обёртки, зеркало str-пути); `BoxIter[T]` помечен `value` → wrapper-record на адаптер
  **5 → 0** heap-allocs (verify: `grep nova_alloc(sizeof(Nova_BoxIter` = 0 во всех
  `plan153_2/*.c`). Codegen-контракт: `receiver_c_type` → `NovaValue_<short>*` (D226
  always-ptr, order-free helpers), dispatch через `prepare_method_recv`, fn-field accessor
  по value-ness, return-inference strip `NovaValue_` перед `Nova_`. **Stage 2
  (generic-over-source, commit `515de574`):** zero-cost generic-over-source
  слой `collections.vec_iter_zc` реализован и зашиплен (Plan 153.2 Ф.2). Каждый адаптер —
  свой generic-over-source `value`-рекорд (`MapIter[I, T, U]` / `FilterIter[I, T]` /
  `FilterMapIter[I, T, U]`), держащий upstream-итератор **инлайн** полем `src I` (не
  boxed `step`-замыкание); `@next()` диспетчит `(@src).next()` СТАТИЧЕСКИМ
  мономорфным вызовом. Цепочка `v.ziter().zmap(f).zfilter(p).zcollect()` мономорфизируется
  в один вложенный конкретный тип (`FilterIter[MapIter[VecIter[int], int, int], int]`),
  каждый `next()` инлайнится до базового `VecIter.next()`. **Измерено** на каноничной
  цепочке `map().filter().collect()`: per-adapter heap-allocs **3 → 0** (убраны
  `nova_lambda_env` + `_box_src` source-box + `NovaClosBase` step-thunk на каждый адаптер),
  source-box (`_box_src`) **9 → 0**, per-element `step()` fn-ptr индирекция убрана целиком
  (источник зовётся статически). **Compiler-changes** (все в `emit_c.rs`): value-aware
  `apply_type_subst_to_ref` (nested-generic arg prefix), depth-aware mono-args splitter
  (`split_top_level_mono_args` + registry-backed `mono_type_args_of` — раньше naive
  `split("__")` рвал вложенный `____`), recursive `erased_type_ref_c` placeholder check,
  value-gated nested-placeholder drain guard. **Остаток** (honest): callback `f`/`pred`
  всё ещё boxed-closure-поле (`void* f` + `NOVA_CLOS_CALL`) — Rust-style инлайн `f` требует
  closures-as-mono-types (env как конкретный type-param), это бóльший лифт →
  `[M-153.2-closure-as-mono-type]`. `take`/`skip`/`enumerate` (stateful / tuple-элемент)
  пока на boxed-fluent `vec_lazy` surface; портирование — wiring, не новая
  compiler-capability. boxed-fluent `vec_lazy` (`BoxIter`) сохранён как closure-fluent
  альтернатива (одна эрейзнутая курсор-форма, ценой ~3 alloc/адаптер).
- **`[M-153.2-closure-as-mono-type]`** (planned, P3, **perf-only**, home **Plan 153.2**):
  в zero-cost `vec_iter_zc` callback-поля `f fn(T)->U` / `pred fn(T)->bool` остаются
  boxed-замыканиями (`void*` + `NOVA_CLOS_CALL`), т.е. вызов мэппера на элемент идёт через
  fn-ptr индирекцию. Rust-style полный инлайн требует мономорфизации замыкания как
  конкретного type-param с запечённым env-типом (closures-as-mono-types). Это отдельный
  крупный лифт поверх Stage 2 (Stage 1 убрал record-alloc, Stage 2 — source-box + step-
  индирекцию, этот маркер — последний closure-box).
- **`[M-153.2-tuple-elem-adapter]`** ✅ **CLOSED 2026-06-16 (commit `d505c0e5`)**.
  Root: generic method dispatch built `type_subst={T:nova_int, B:nova_int}` for `BoxIter[A] @zip[B]`
  but return type `BoxIter[(A,B)]` referenced local alias `A` absent from subst → `type_ref_to_c(Tuple[A,B])`
  fell to erased `_NovaTuple2` → CC-FAIL type mismatch. Fix: in non-nested receiver dispatch path, after
  tmpl.generics bind, structurally bind receiver_ty local typevars via `infer_type_param_binding`, adding
  NEW names only. Tests: `plan153_2/zip_basic` + `plan153_2/zip_min` PASS (9/9 plan153_2 total).
- **`[M-153.2-flat-map-inner-option]`** ✅ **CLOSED 2026-06-16 (commit `d505c0e5`)**.
  Root: NovaOpt typedef for `BoxIter[T]` payload (NovaValue_ by-value) was emitted before the generic
  struct body it referenced → CC-FAIL "field has incomplete type". Fix: new `novaopt_vr_typedefs_buf`
  spliced after `/*__GENERIC_TYPE_DEFS__*/` marker; VR-payload routing in `register_novaopt_decl`/
  `register_novaopt_decl_forced`. Test: `plan153_2/flat_map_basic` PASS.
- **`[M-153.2-requires-cc-fail]`** (🟡 **OPEN — compiler bug, workaround committed**, 2026-06-16,
  P2, home **Plan 153.2**): `requires` contract on a method returning `BoxIter[T]` (a value-record)
  causes codegen to emit `return 0` instead of a zero-initialized struct → CC-FAIL from C compiler
  (`invalid initializer` / type mismatch). Root: the contract enforcement codegen path hardcodes
  `return 0` for early-exit on violation, which is valid for scalar/pointer returns but not for
  value-record returns. Workaround committed in `78e75f5b`: `step_by_zero_neg` uses a `test {}`
  block (which expects runtime panic) instead of a top-level `fn main` form that triggered the
  bug. The root compiler bug is still present — any method with `requires` that returns a
  value-record will CC-FAIL in a `fn main` harness. Фикс: в codegen contract-enforcement path
  emit `return (ReturnType){0}` (C99 zero-initializer) instead of bare `return 0` when the
  return type is a value-record (check `value_record_names.contains`).
- **`[M-153.2-iter-phase-b]`** ✅ **CLOSED (2026-06-16, коммит `d505c0e5`)**:
  - ✅ **`step_by(n)`**: BoxIter (vec_lazy) + zero-cost `StepByIter` (vec_iter). Contract `requires n > 0`. Тесты: `plan153_2/phase_b_lazy` + `plan153_2_zc/step_by_zc` PASS.
  - ✅ **`chain(other)`**: BoxIter (vec_lazy). Тест: `plan153_2/phase_b_lazy` PASS.
  - ✅ **`zip`**: реализован + тесты `plan153_2/zip_basic` + `plan153_2/zip_min` PASS. Блокер `[M-153.2-tuple-elem-adapter]` ЗАКРЫТ.
  - ✅ **`flat_map`**: реализован + тест `plan153_2/flat_map_basic` PASS. Блокер `[M-153.2-flat-map-inner-option]` ЗАКРЫТ.
  - ❌ Остаток: `unzip`/`flatten`/`scan`/`inspect`/`take_while`/`skip_while`/`peekable`/`min_by[_key]`/`max_by[_key]`/`partition`/`chunk_by`/`into_iter` + мут-итерация `for mut x` (Q-iter-mut).
  `FromIterator`/collect-target ✅ закрыт (Plan 153.6 / D264).
  Phase A (map/filter/filter_map/enumerate/take/skip + 13 терминаторов + `@collect_set`)
  закрыта и протестирована (`plan153_2/` 4/4, `plan153_6/collect_target` 12/12).
- **`[M-153.2-drop-z-prefix]`** ✅ **CLOSED (Plan 164 Ф.3+Ф.4, 2026-06-16, commits `af33bc76`+`70079788`).** Compiler-баги `[M-codegen-blanket-generic-param-order]` и receiver-compatibility все закрыты. Терминаторы `@znth`→`@nth`, `@zmin`→`@min`, `@zmax`→`@max` переименованы. Новая receiver-compatibility dispatch rule (D285 §3) обеспечивает, что blanket-метод на совместимом типе выигрывает над concrete-методом на несовпадающем. Остаток (не блокирует): полный sweep `@zmap`→`@map`/`@zfilter`→`@filter`/`@zcollect`→`@collect` и `vec_iter_zc.nv`→`vec_iter.nv` — пост-merge cleanup sweep. Ключевой workaround (z-prefix на конфликтующих терминаторах) СНЯТ.

## Follow-up: Plan 153.3 (sort & search)
- **`[M-153.3-sort-unstable-inplace]`** ✅ **RESOLVED** (commit `468bccf5`): `@sort_unstable[_by]
  [_by_key]` переведены с alias-стабильного на **настоящий in-place heapsort** (O(n log n) worst,
  O(1) extra — даёт `_unstable` его смысл: без scratch-буфера и без quicksort O(n²)-обрыва).
  Тест `plan153_3/heapsort_rigor` 5/5.
- **`[M-153.3-sort-pdqsort]`** ✅ **CLOSED (Plan 153.3.1, 2026-06-18)**: pdqsort реализован в
  `@sort_unstable*` — median-of-3 pivot + Lomuto partition + ins-sort(n≤16) + heapsort depth-guard
  (2·ilog2(n)). O(n log n) worst, O(log n) stack. Heapsort сохранён как depth-guard fallback.
- **`[M-153-select-nth]`** ✅ **RESOLVED** (commit `468bccf5`): `@select_nth_unstable(k)` реализован
  как **introselect** — median-of-three quickselect (O(n) средн.) + depth-guard fallback на
  heapsort (O(n log n) worst, без O(n²)), контракт `k ∈ [0,len)`. Тест `plan153_3/select_nth` 4/4
  + OOB-panic neg.
- **`[M-153-result-eq-literal-expected-type]`** ✅ **RESOLVED** (codegen re-emit): `result == Ok(x)`
  / `== Err(x)` для Result с **non-default E** (≠`str`; напр. `binary_search`→`Result[int,int]`)
  теперь работает. Был баг: голый литерал `Ok(x)` инферил `E=str` (codegen-дефолт `emit_c.rs:5172`,
  т.к. чекер оставляет variant-ctor без типа by design — тест `infer_call_sum_variant_stays_unknown`)
  → `binary_search() == Ok(2)` сравнивал два разных `NovaRes_<…>` → CC-FAIL. **Фикс** (codegen-local,
  `==`-NovaRes_-бранч): если типы операндов `Eq/Neq` расходятся и одна сторона — голый `Ok/Err`-
  литерал, переэмитить её под concrete `NovaRes_<n>` другой стороны (`reemit_result_variant_as` +
  `expr_is_result_ctor`, payload-cast). Тест `plan153_3/result_eq_literal` (binary_search ==Ok/Err
  non-default-E + explicit Result[int,int] + default-E Result[int,str] не сломан). Частный случай
  отложенного-с-мая `Q-overload-result-type` (general expected-type propagation для overload-резолва
  `@into` остаётся открытым — см. Q).
- **`[M-153.x-std-sort-consolidate]`** ✅ **PARTIAL / RESOLVED (binary_search)** (2026-06-14): после
  Plan 153.3 prelude `Vec[T Compare] @binary_search -> Result[int,int]` (`collections.vec/access.nv`)
  стал ПЕРЕКРЫВАТЬ `std.sort`-овский `[]int @binary_search -> Option[int]` (D239 `[]T ≡ Vec[T]` →
  оба = метод на `Vec[int]`, prelude побеждает резолв) → `plan91/sort_basic` (+ `plan91_fe4/
  sort_aggregate_pipeline`, `plan91_fe5/sort_realistic`) CC-FAIL на `binary_search() == Some(_)`
  (`NovaRes_*` vs `NovaOpt_*`). **Фикс:** канон = Vec `Result`-форма; `[]int @binary_search` УДАЛЁН
  из `std/sort.nv`; call-sites мигрированы на `== Ok(i)` / `== Err(insertion_point)` (`sort_basic`,
  `sort_aggregate_pipeline`, `sort_realistic`, `plan91_fe4/neg/edge_and_error_paths`). plan91 2/0,
  plan153_3 7/0; **бонусом починены 2 ранее-красных** (sort_aggregate_pipeline, sort_realistic).
  **`@sort`/`@sort_by`/`@min`/`@max` НАМЕРЕННО ОСТАВЛЕНЫ в `std.sort`** (не return-type-конфликтят;
  их `[]int`-exact-receiver сигнатуры всё ещё ТРЕБУЮТСЯ — см. блокер ниже). Generic `*_of`-семейство
  + `@sum`/`@product` нетронуты.
- **`[M-153.x-array-new-not-vec]`** (planned, **codegen-блокер полной консолидации std.sort → Vec**):
  `[]int.new()` / `[]int.with_capacity()` лоуэрятся в ЛЕГАСИ runtime-тип `NovaArray_T` (НЕ `Vec[T]`),
  тогда как литерал `[1,2,3]` → настоящий `Vec____nova_int`. Вызов prelude Vec `mut @`-метода
  (`@sort`/`@reverse`) на `NovaArray`-ресивере мис-диспатчится в erased-generic `Nova_Vec_method_*`
  (element-width стёрт до `void*`) → метод **молча no-op'ит** оригинал (`a.sort()` не мутирует `a`).
  Поэтому `std.sort` `[]int @sort`/`@sort_by` нельзя удалить, не отрегрессив каждый `[]int.new()…sort()`
  (напр. `plan91_7/sort_chain_test`). Полная консолидация sort/sort_by/min/max в Vec-канон + перенос
  eager-`@min`/`@max` в `collections.vec` ГЕЙТИТСЯ на этом фиксе (роутинг `[]T`-static-конструкторов
  на `Vec[T]`). Связано с `[M-method-resolution-registry-inconsistency]` (last-wins реестр; тот же
  класс что pre-existing `empty.min()` → `Nova_Duration_method_min` коллизия в `plan91_fe4/neg`).

## Follow-up: Plan 153.4 (slices — value-record element typedef)
- **`[M-153.4-chunks-windows-lazy]`** ✅ **CLOSED 2026-06-14** (Plan 153.4-B, home **Plan 153.4 / D262**):
  `@chunks(n)` / `@chunks_exact(n)` / `@rchunks(n)` / `@windows(n)` реализованы **ЛЕНИВО** (Rust/Kotlin-
  стиль, БЕЗ аллокации внешнего `[][]T`-Vec) поверх ленивой инфры Plan 153.2 — каждый = инстанс-метод
  `Vec[T] @… -> BoxIter[Self]` в `std/collections/vec_lazy.nv` (sibling-файл, не prelude `vec/`: bodies
  форвардят capturing-closure → generics-leak, как все адаптеры), yield'ящий zero-copy `[]T`-views
  (`src[a..b]`, `cap==len`) на том же буфере; `collect()` материализует `[][]T` только по требованию,
  `chunks(n).map(|w| …)`/`fold`/`count`/`for_each` — без аллокации внешнего Vec вовсе. Контракт `n > 0`
  (`requires`, runtime-panic). Заметка по codegen: early-exhaustion возвращает `ro done Option[Self] =
  None` (типизированный локал), т.к. bare `return None` в closure с КОНКРЕТНЫМ элементом `Vec[T]` (не
  свободный generic) монофится в дефолтный `Option[<elem>]` и расходится с `BoxIter[Self]` step-return
  (тот же класс что `[M-153.2-tuple-elem-adapter]`) — БЕЗ compiler-фикса, чисто аннотацией локала.
  Фикстуры: `plan153_4/chunks_windows` (23 test-блока: even/odd split, short remainder, exact-drop,
  reverse rchunks, overlapping windows, n==1, n>=len, empty, lazy map/fold/count) + 4 негатива
  `chunks_zero_neg`/`chunks_exact_zero_neg`/`rchunks_zero_neg`/`windows_neg` (EXPECT_RUNTIME_PANIC
  requires). Верификация: plan153_4 7/0, plan153_2 4/0, plan96 23/0, plan153_0 4/0, plan153_1 7/0,
  basics 8/0, plan131 28/0, plan138 10/0 = 0 регрессий. **153.4 (A+B) ЗАКРЫТ ЦЕЛИКОМ.**
- **`[M-153.4-vec-value-record-field-access]`** (planned, P2, home **Plan 153.4 / Plan 96 H3**):
  slice-каст value-record (`ranges[1..3]` на `[]Range`) **ЗАКРЫТ** (emit_c.rs:19052 un-mangle
  `_p`→`*` для element-cast; передан агентом 152-sweep'а, был pre-existing, НЕ регрессия 152).
  Остаётся доступ к **ПОЛЮ** элемента value-record — `ranges[0].start` — тот же корень (элемент
  мангатится в `Nova_Range_p`, теряет точный тип), но ДРУГОЙ codepath (element-field-access,
  Plan 96 H3 / array_element_types Ф.4.5). Не входил в acceptance. Применить аналогичный un-mangle.
- **stale neg-message** ✅ **CLOSED 2026-06-14 (Plan 152 sweep)**: plan96/neg_slice_{a_gt_b,
  a_negative,oob_to} ждали `EXPECT_RUNTIME_PANIC array: slice`, а сообщение теперь
  `Vec: slice ... out of bounds` (D239 `[]T`→`Vec` rename, emit_c.rs:19045). Директивы
  выровнены на `Vec: slice`. plan96 23/0. Program behaviour был корректен (OOB → panic).

## Follow-up: Plan 160 (module-level field privacy)

- **`[M-160-per-field-priv-module]`** ✅ **CLOSED 2026-06-15** — field-level bare `priv` теперь = module-private (симметрично type-level; commit `b87ffeef`). `priv(type)` на field-level = type-private. Синтаксис симметричен полностью.
- **`[M-160-methods-module-visibility]`** (Q, floating) — все методы без `export` сейчас module-private по умолчанию (не public). `priv(type)` на методе — потенциальное расширение (type-private method). → [Q-method-type-private](../../spec/open-questions.md#q-method-type-private) (добавлено 2026-06-16); реализовать при явной потребности.
- **`[M-160-named-tuple-priv]`** ✅ **DEFERRED (won't do, 2026-06-15)** — named tuples всегда с публичными полями; priv/priv(type) на tuple-fields откладывается до явной необходимости.
- **`[M-160-pattern-match-module-priv]`** ✅ **CLOSED 2026-06-15** — smoke-test `ro { id } = j` из другого модуля → `E_FIELD_MODULE_PRIVATE`; `nova_tests/plan160/neg_pattern_outside.nv` 1/1 PASS. Чекер покрывал path (types/mod.rs:5470), теперь зафиксировано тестом.

## Follow-up: Plan 124.6 (escape hatches — implicit test access + `#[test_access]` + `#[pub_to]`)

✅ **CLOSED 2026-06-16** — D224 AMENDED; design изменён от fn-level/field-level к test-block-level + type-level; 15/15 PASS; merge `ea1bc7c7`.

Нет открытых маркеров. Потенциальный followup при необходимости:
- `#[pub_to]` per-field гранулярность (сейчас type-level = все `priv(type)` поля сразу)

## Конвенция
- **Planned** маркер → Followups своего плана (+ индекс-строка здесь с home).
- **Floating** (нет плана) → здесь полностью.
- Закрыл → убрал строку (история в simplifications.md). Держим только живое.

## Follow-up: Plan 91.12 / 91.13 (std/net V2 algebraic effects + bytes-FFI + DNS + consume value types)

- ~~**`[M-91.12-bytes-ffi]`**~~ ✅ **CLOSED 2026-06-16** — `str @as_ptr() -> *u8` добавлен (`std/runtime/string/core.nv`); DNS handler использует `host.as_ptr()` + `host.byte_len()`. D294. Тест: `net_v2_str_as_ptr_ok` 5/0 PASS.
- ~~**`[M-91.12-async-dns]`**~~ ✅ **CLOSED 2026-06-16** — DnsNet раскомментирован (`std/net/effect.nv`), `real_dns_net()` реализован (`std/net/dns.nv`), C-side `dns_lookup`/`dns_addr_at` через `uv_getaddrinfo` + TLS (`compiler-codegen/nova_rt/net.c`). D295. `SocketAddr.lookup()` wrapper обходит vtable type-erasure. Тест: `net_v2_dns_smoke` 6/0 PASS. 21/0 plan91_12 PASS.
- ~~**`[M-91.13-dns-iter-boxing]`**~~ ✅ **CLOSED 2026-06-16** — `is_generic_stub_c` fix (`&& !name.contains("____")`) + DnsNet V2 `[]SocketAddr` API. Vtable erasure устранена; `real_dns_net()` строит Vec через `dns_addr_at(0..count)`; `mock_dns_net()` возвращает `Ok([loopback(0)])`. 21/0 plan91_12 PASS.
- ~~**`[M-91.13-real-dns-integration-test]`**~~ ✅ **CLOSED 2026-06-16** — `net_v2_dns_real_slow.nv` добавлен (`_slow` suffix, `NOVA_SLOW_TESTS=1` opt-in); `assert(r.is_ok())` с реальным `localhost` resolver.
- **`[M-91.12-double-close-static]`** — double-close через effect-dispatch не ловится checker'ом для `mut`-binding value types (только `consume`-binding consume-types отслеживаются). → Future Plan.
- **`[M-91.12-real_addr_net-naming]`** — рассмотреть `sys_tcp_net/sys_addr_net` vs `real_*` naming. → Future API review.
- **`[M-91.16-tcp-split]`** — ✅ CLOSED 2026-06-17 (Plan 91.16, D301). `TcpReadHalf`/`TcpWriteHalf` consume-split реализован: независимые C-side park-слоты (`read_scope`/`read_slot` vs `write_scope`/`write_slot`), atomic `split_refcount` на C-handle, mock + real-network тесты PASS.
- **`[M-91.12-split-halves]`** (TCP) — ✅ CLOSED 2026-06-17 (Plan 91.16, D301). См. выше.
- **`[M-91.16-tuple-consume-binding]`** — consume-tracking не пробрасывается через tuple-destructuring: `consume (rd, wr) = s.split()` → parse error («unexpected `consume`»), а `mut (rd, wr)` не отслеживается на double-consume. → double-close одной из split-половин НЕ ловится компилятором (refcount защищает на runtime). Нужна поддержка `consume`-binding на tuple-pattern в парсере + consume-checker. → Future Plan. (тот же класс, что `[M-91.12-double-close-static]`.)
- **`[M-net-udp-two-fiber-race]`** — 🔴 OPEN (P2, обнаружено 2026-06-17, Plan 91.16 final regression). `net_v2_udp_two_fiber_slow` (plan91_12) интермиттентно зависает (~2/3 запусков timeout ~45-73s даже в isolation `--jobs 1`, ~1/3 PASS). Класс: park/wake race на UDP-loopback двух concurrent-фиберов (один `recv_from` паркуется, другой `send_to`; датаграмма теряется или wake не приходит). НЕ регрессия Plan 91.15/91.16: UDP-рантайм в `compiler-codegen/nova_rt/net.c` byte-identical к base `ccca04f6` (0 UDP-строк изменено), `std/net/udp.nv` — только снятие `Blocking`-аннотации (без runtime-эффекта). Корень в shared M:N park/wake (ср. `[reference-mn-race-case-study]`). → Future Plan (UDP recv/send wake-ordering hardening).

## Plan 91.16 — TCP split: TcpReadHalf + TcpWriteHalf ✅ CLOSED 2026-06-17

По образцу UDP split (Plan 166 / D377, ex-D298). `TcpStream consume @split() -> (TcpReadHalf, TcpWriteHalf)`. Atomic refcount (`split_refcount`) на C-handle, оба half — consume value, независимые park-слоты для concurrent r/w. Также добавлен `TcpStream @write_all` + `TcpWriteHalf @write_all` (закрывает `[M-91.15-write-all]`). Spec: D301. Тесты: `nova_tests/plan91_16/` (mock + `_slow` real-network + stream-after-split neg). → маркер `[M-91.16-tcp-split]` CLOSED.

## Plan 91.15 — std/net API polish ✅ CLOSED 2026-06-17

### P0 — Удалить retracted фичу (blocking {} / Blocking) ✅ DONE

| Маркер | Статус | Home | Действие |
|---|---|---|---|
| `[M-91.15-remove-blocking]` | ✅ CLOSED 2026-06-17 (P0). Blocking убран из TcpNet/UdpNet сигнатур; compiler registry + checker arm. | Plan 91.15 P0 | ✅ done |

### P1 — Критические ✅ DONE

| Маркер | Статус | Home | Действие |
|---|---|---|---|
| `[M-91.15-write-all]` | ✅ CLOSED 2026-06-17 (P1). TcpStream/TcpWriteHalf.write_all() — C-backed. | Plan 91.15 P1 | ✅ done |
| `[M-91.15-eof-semantics]` | ✅ CLOSED 2026-06-17 (P1). NetError.Eof при TCP peer-close. | Plan 91.15 P1 | ✅ done |
| `[M-91.15-neterror-to-str]` | ✅ CLOSED 2026-06-17 (P1). NetError @to_str() 14 variants. | Plan 91.15 P1 | ✅ done |
| `[M-91.15-host-str-rename]` | ✅ CLOSED 2026-06-17 (P1). SocketAddr.ip() renamed from host_str(). | Plan 91.15 P1 | ✅ done |

### P2 — Важные дополнения ✅ DONE

| Маркер | Статус | Home | Действие |
|---|---|---|---|
| `[M-91.15-permission-denied]` | ✅ CLOSED 2026-06-17 (P2). NetError.PermissionDenied + UV_EACCES normalization. | Plan 91.15 P2 | ✅ done |
| `[M-91.15-connection-reset]` | ✅ CLOSED 2026-06-17 (P2). NetError.ConnectionReset + UV_ECONNRESET normalization. | Plan 91.15 P2 | ✅ done |
| `[M-91.15-connect-timeout]` | OPEN P3. TCP connect-timeout API. | plan-91.15 Followups | P3 |
| `[M-91.15-read-bytes]` | OPEN P3. read_bytes(n int) — guaranteed binary read. | plan-91.15 Followups | P3 |
| `[M-91.15-effect-prefix-consistency]` | OPEN P3. Convention documented in effect.nv; no renames (zero user impact, high churn). | plan-91.15 Followups | P3 |

### P3 — Полезно иметь (DEFERRED)

| Маркер | Статус | Home | Действие |
|---|---|---|---|
| `[M-91.15-read-exact]` | OPEN P3. read_exact() — fixed-length framing. | plan-91.15 Followups | P3 |
| `[M-91.15-shutdown-write]` | OPEN P3. shutdown_write() — half-close TCP write side. | plan-91.15 Followups | P3 |
| `[M-91.15-so-reuseport]` | OPEN P3. SO_REUSEPORT for multi-listener load-balancing. | plan-91.15 Followups | P3 |
| `[M-91.15-udp-multicast]` | OPEN P3. UDP multicast join/leave API. | plan-91.15 Followups | P3 |

[M-118.6-tuple-field-escape] tuple field chain-root tracking — &tuple.N escape analysis.

## Follow-up: Plan 106 (if/while && guard)

| Маркер | Статус | Home | Действие |
|---|---|---|---|
| `[M-106-if-guard]` | ✅ CLOSED 2026-06-17. && guard в if/while pattern-bind. | Plan 106 | ✅ done |
| `[M-106-if-let-chain-multi]` | OPEN P3. Множественные let-паттерны через `&&` (Rust let-chains): `if Some(x)=a && Some(y)=f(x) && x+y>10`. Единственный `consider` в этой зоне (survey 2026-07-02); guard уже покрывает ~90%, выигрыш над nested-`if` мал → делать opportunistic. | [spec/open-questions Q-if-let-chain-multi](../../spec/open-questions.md) | P3 |

## Plan 104.10 Ф.0.5 — diagnostic pipeline correctness (impl RESOLVED 2026-07-03; spec close-out Ф.14)

Sub-markers of `[M-104.10-diag-pipeline-correctness]` / `[M-spec-nova-lsp-conformance-audit]`.
Fixes landed in `nova-lsp` (`compiler.rs`, `diagnostic_mapping.rs`, `completion.rs`);
pos+neg+parity fixtures in `nova-lsp/tests/diagnostic_pipeline.rs`. Formal close-out
(spec D-blocks / plan status) deferred to Ф.14.

| Маркер | Статус | Home | Действие |
|---|---|---|---|
| `[M-104.10-import-diag-swallowed]` | ✅ RESOLVED (Ф.0.5). `check_source_inner` surfaces import-resolution errors as diagnostics (`import resolution: …`) instead of `let _ = …`; real cause shown, not downstream «unknown type». | Plan 104.10 Ф.0.5 | close-out Ф.14 |
| `[M-104.10-degraded-cu-red]` | ✅ RESOLVED (Ф.0.5). Best-effort repo fallback (nova.toml → LSP workspace root → entry-dir) + scratch entry for unsaved buffers → `peer_files` populated (prelude + folder-module peers) → 0 false-red on `print`/`Vec`/peer symbols. | Plan 104.10 Ф.0.5 | close-out Ф.14 |
| `[M-104.10-lsp-cmd-check-drift]` | ✅ RESOLVED (Ф.0.5). LSP check-вход сведён к `nova check` пайплайну: `resolve_imports` + `number_exprs` + `collect_all_signatures` + `check_module_with_sig_table` (Plan 162.2 suppression). Резолвит Q-104-4. | Plan 104.10 Ф.0.5 | close-out Ф.14 |
| `[M-104.10-diag-numeric-codes]` | ✅ RESOLVED (Ф.0.5). `extract_error_code` теперь распознаёт legacy числовые `[Ennnn]` (не только symbolic `[E_…]`) → code-action dispatch срабатывает по ним. | Plan 104.10 Ф.0.5 | close-out Ф.14 |
| `[M-104.10-hardcode-lists]` | ✅ RESOLVED (Ф.5, 2026-07-03). Все хардкод-списки удалены: `STD_MODULES` → FS-скан `stdlib_index.rs::StdlibIndex` (кэш в `WorkspaceState::stdlib_index`); `code_actions.rs known_stdlib_type_module/protocol_import` → `StdlibIndex::{type_module,protocol_module}`; `auto_derivable_protocols` → `auto_derive::builtin_protocol_names()` (убраны pre-D237 имена); `rename.rs NOVA_KEYWORDS` → `lexer::is_reserved_keyword`. compiler-conventions §3 удовлетворён. | Plan 104.10 Ф.0.5 → Ф.5 | close-out Ф.14 |
| `[M-104.10-cli-degraded-cu-red]` | ✅ RESOLVED (221.1 Б1, ветка `p-fix-104-diag`). CLI-двойник `[M-104.10-degraded-cu-red]`, оставшийся непочиненным при LSP-волне: `check_one_file` (`nova-cli/src/main.rs`) гейтовал ВЕСЬ import/prelude-резолв за голым CWD-заякоренным `find_repo_root()` — при неудаче (нет ancestor `nova.toml` от CWD; напр. проверка standalone-файла вне проекта) резолв ПОЛНОСТЬЮ пропускался (не деградировал — вообще не пытался), и ЛЮБОЙ prelude-символ (`println`/`Vec`/…) ложно краснел `undefined identifier` для валидного кода. Хуже: `NOVA_STD_PATH`-override (Plan 91.9, штатный механизм для standalone-режима) при этом молча игнорировался — гейт даже не доходил до `resolve_std_path`. Репро подтверждено (RED): standalone `.nv` с `println` вне любого `nova.toml`, даже с `NOVA_STD_PATH`, указывающим на настоящий std/, — ложный `undefined identifier`. Фикс: best-effort repo-якорь (зеркало LSP-фолбэка) — CWD-`nova.toml` → entry-файл-заякоренный `find_repo_root_from(path)` (тот же helper, что уже использует `embed_resolve` парой строк ниже в той же функции) → директория entry-файла; `resolve_imports_inline_ex` безопасен при несуществующем `stdlib_dir` (prelude-guard в `imports.rs` — no-op), так фикс только улучшает резолв, никогда не регрессирует. Гейт: RED→GREEN на репро; regression-тесты `nova-cli/tests/check_degraded_cu_prelude.rs` (POS: prelude-символ резолвится через `NOVA_STD_PATH` в standalone-файле; NEG: настоящая ошибка `undefined_symbol_xyz` не глушится) — 2/2 PASS; `nova-cli` full suite 142/142 (+2 новых) PASS, `edition_resolve` 2 pre-existing RED (стейл `let`-фикстуры, Plan 114/D184, воспроизведено на pristine main-бинаре — НЕ регресс этой волны); `nova-lsp diagnostic_pipeline` не задет (нет правок в nova-lsp). | Plan 104.10 Ф.0.5 / 221.1 Б1 | ✅ DONE |
| `[M-104.10-lsp-cwd-anchor]` | Follow-up P3 (test-only). Path-free обёртки `completion_for`/`method_items`/`import_items` резолвят stdlib через `current_dir()` discovery для тестов; LSP-сервер всегда передаёт реальный путь + кэш `StdlibIndex` (`completion_for_doc`), на CWD не опирается. | Plan 104.10 Ф.5 | P3 |
| `[M-104.10-lsp-resolve-method-doc]` | Follow-up P4 (косметика). Ф.13 lazy resolve: method-doc кладётся в `data` (уже вычислен при module-resolve для списка) и переносится в `documentation` на resolve — рендеринг отложен, но wire-payload немногих method-item'ов не уменьшается. Статические семейства (keyword/snippet/prelude/import) genuinely пере-выводятся из таблиц → тяжёлый текст не едет в initial-ответе. Альтернатива (пере-резолв per-item по locator) дороже. | Plan 104.10 Ф.13 | P4 |

## LSP client lifecycle — didClose-after-stream-destroyed (2026-07-22)

| Маркер | Статус | Home | Действие |
|---|---|---|---|
| `[M-lsp-didclose-after-stream-destroyed]` | ✅ CLOSED same session (ветка `p-fix-lsp-lifecycle`, sonnet). Владелец увидел рецидив `[Error] Sending document notification textDocument/didClose failed. Error: Cannot call write after a stream was destroyed` (Output > Nova LSP) ПОСЛЕ «workspace index ready, 3186 files» — т.е. уже теплый, простаивающий сервер, вне окна холодного скана. Прошлая волна (`p-fix-lsp-write-destroyed`, merge `70e34308e`/`0c12dc70e`, 2026-07-20, см. `docs/plans/wip/lsp-write-fix-notes.md`) чинила ТОЛЬКО серверную причину форс-килла (холодный скан без cancel-гейта держал процесс живым дольше 2000мс grace-периода client.stop()) — клиентская сторона (`editors/vscode/client/extension.ts`) НИКОГДА не трогалась (явно отложено в её же заметках, п.4 «не смотрел вообще»). Рецидив на уже-тёплом сервере (скан давно завершён) доказывает: root — это САМОСТОЯТЕЛЬНАЯ, независимая от скорости выхода сервера гонка ВНУТРИ `vscode-languageclient` 9.0.1 (`node_modules/vscode-languageclient/lib/common/{textSynchronization,client}.js`, third-party — не патчим): `DidOpenTextDocumentFeature`/`DidChangeTextDocumentFeature`/`DidSaveTextDocumentFeature`/`DidCloseTextDocumentFeature` шлют `client.sendNotification` без проверки `client.state === State.Running`, а `shutdown()`/`handleConnectionClosed()` не ждут in-flight отправки перед `connection.end()/dispose()` — событие закрытия документа, попавшее в это окно (наш `stop()`/restart на config-change, `deactivate()`, каскад закрытия документов при reload окна, независимый выход процесса сервера), долетает до уже разрушенного потока; библиотечный неохраняемый `.catch(() => this._client.error(...))` печатает это как top-level Error, хотя это ожидаемая гонка закрытия, не функциональный сбой. **Фикс (только клиент — единственный доступный рычаг над сторонней зависимостью):** `guardedDocumentSend` middleware (`didOpen`/`didChange`/`didSave`/`didClose`) в `clientOptions.middleware` — (1) пропускает отправку, если `client.state !== State.Running`; (2) если отправка всё же пошла и упала (state был Running, но соединение умерло между проверкой и записью), гасит reject сама и логирует на info-уровне вместо Error — библиотечный внешний `.catch` ловить уже нечего. Плюс defense-in-depth: `client.stop(5000)` вместо дефолтных 2000мс (шире grace-период → меньше шансов вообще дойти до форс-килла). Серверная сторона (`nova-lsp/src/server.rs`, `state.rs::shutting_down`) проверена повторно — гейт уже покрывает все фоновые пути (18 site'ов `is_shutting_down()`), с 2026-07-20 не менялась (`git log 70e34308e..HEAD -- nova-lsp/` пусто) — не регрессия там, чинить нечего. | worktree `nova-lspfix` | ✅ DONE |

## Plan 104.10 Ф.1 — symbol cache (resolved-module cache; impl DONE 2026-07-03)

Per-URI cache of the fully-resolved module (parse + import-inline + type-check
with `expr_types`) in `WorkspaceState::resolved_cache` (`state.rs`); built via
`provenance::resolve_module_for_ide` (`provenance.rs`). Cache hit by document
version; evicted on `didClose` (`server.rs`). pos+neg+edge+perf tests in
`state.rs` (`f1_*`).

| Маркер | Статус | Home | Действие |
|---|---|---|---|
| `[M-104.10-dependent-invalidation]` | 🟡 PARTIAL (Ф.1; Ф.18 добавил примитив). Кеш инвалидируется по СВОЕМУ `uri`+`version`: `didChange` A перестраивает A, но кеши импортёров A остаются до их собственного edit/close. Ф.18 добавил `state::invalidate_all_resolved` (корректный coarse superset) и подключил его для ВНЕШНИХ событий (`didChangeWatchedFiles`/rename) — external change больше не оставляет stale. Остаётся open для per-edit `didChange`-пути (точный module-graph reverse-dep обход при интерактивной правке импортёра, урок zls). См. `[M-104.10-watch-reverse-deps]`. | Plan 104.10 Ф.1/Ф.18 | P3 |

## Follow-up: Plan 104.9 (nova-lsp language-sync + close-out)

| Маркер | Статус | Home | Действие |
|---|---|---|---|
| `[M-104.9-completion-language-sync]` | ✅ CLOSED 2026-06-17. completion.rs/code_actions.rs синхронизированы с языком. | Plan 104.9 | ✅ done |
| `[M-104.9-dynamic-method-completion]` | ✅ CLOSED Plan 104.10 Ф.5 (2026-07-03). Type-driven completion из compiler `ResolvedModule` (`method_items_typed`); статические таблицы удалены. | plan-104.9 Followups | ✅ done |
| `[M-104.5-suggestion-field-wiring]` | OPEN P3. CodeAction edit — re-scan vs compiler Suggestion.span. | plan-104.5 Followups | P3 |

## Follow-up: Plan 104.4 (documentSymbol + workspaceSymbol + references)

✅ **CLOSED 2026-06-16** — branch `plan-104-4`, commit `8b3e1903`; 86+15 PASS.

Open V1 markers (gated on type-checker resolver API in Plan 104.2):
- **`[M-104.4-refs-incremental-index]`** — ✅ RESOLVED (Plan 104.10 Ф.12, 2026-07-04). Полноценный инкрементальный in-memory индекс `name → [(uri, span)]` (`symbols.rs::ReferencesIndex`) заменил per-request full-FS-скан; обновление на didOpen/didChange/watch/rename, фон-индекс всего workspace + ленивый cold-prime. On-disk persistence — новый `[M-104.10-persistent-index]`.
- **`[M-104.10-persistent-index]`** — ✅ RESOLVED ([Plan 215](215-lsp-index-cache.md), 2026-07-19). On-disk персистентный кэш `workspace/symbol` + `references`-индекса (`nova-lsp/src/index_cache.rs`, `target/nova-lsp-cache/index-v1.json`, инвалидация по (mtime,size), версия схемы → молчаливый фолбэк на порче/несовместимости). Тёплый старт ~6x быстрее холодного на реальном workspace (7.0с vs 42.4с, 3093 файла); точечная инвалидация подтверждена. Diagnostics (`check_workspace`) остаются вне объёма — см. `[M-104.10-dependent-invalidation]`.
- **`[M-104.10-folding-plain-comments]`** 🟡 OPEN (P3, Ф.16). Плоские `//`-line-comment-run'ы не сворачиваются (лексер их отбрасывает, не доходят до AST); doc-comment'ы (единственная AST-представленная multi-line comment-форма) сворачиваются. Nova не имеет block-комментариев. Файл: `nova-lsp/src/folding_range.rs`.
- **`[M-104.10-highlight-lexical-occurrences]`** 🟡 OPEN (P3, Ф.15). documentHighlight резолвит символ семантически, но вхождения для не-локальных находит лексически (word-boundary в scope); полный semantic per-occurrence resolve — при потребности. Файл: `nova-lsp/src/*` (documentHighlight handler).
- **`[M-104.10-inlay-config-granularity]`** 🟡 OPEN (P3, Ф.9). Inlay hints (type + param-name) включаются глобально capability'ей; тонкая per-kind клиентская настройка (только типы / только параметры) — follow-up. Файл: `nova-lsp/src/inlay_hints.rs`.
- **`[M-104.10-organize-imports-namescan]`** 🟡 OPEN (P3, Ф.11). Unused-import detection через whole-word текстовый name-scan (консервативный false-keep возможен); полный semantic usage-граф — при потребности. Файл: `nova-lsp/src/organize_imports.rs`.
- **`[M-104.10-runtest-filter-substring]`** 🟡 OPEN (P3, Ф.20). codeLens run-test дёргает `nova test` с фильтром по имени теста (substring-match может зацепить лишнее при совпадающих префиксах); точный per-test селектор — follow-up. Файл: `nova-lsp/src/*` (codeLens/executeCommand).
- **`[M-104.10-semantic-tokens-scope]`** 🟡 OPEN (P3, Ф.10). Full semantic tokens покрывают fn/type/var/param/field/keyword; отдельные редкие категории (напр. type-param-в-generic-bound контекстно) — точечное расширение legend при потребности. Файл: `nova-lsp/src/semantic_tokens*.rs`.
- **`[M-104.4-workspace-symbol-fuzzy]`** — workspace/symbol uses substring V1 (V2: fuzzy ranking / prefix scoring). Independent of type-checker.
- **`[M-104.4-cross-file-method-nesting]`** — documentSymbol nests methods under type only within same file via receiver name match (V2: cross-file resolver needs Plan 104.2 symbol resolution API).

## Follow-up: Plan 104.5 (LSP Code Actions / Quick-fixes V1)

- **`[M-104.5-suggestion-field-wiring]`** (P2, home **Plan 104.5**) — `Suggestion` struct field в compiler diagnostic не propagated в LSP yet; code_actions.rs парсит сам из message text. Когда compiler добавит machine-readable `Suggestion` поле в DiagnosticResult, LSP should consume it directly without re-parsing. → Plan 104.x или Plan 101 V3.
- **`[M-104.5-multi-edit-rename]`** (P3, home **Plan 104.5**) — fix handlers currently produce single-span TextEdit; multi-edit (e.g., rename generic `T` → `T1` across all occurrences in fn signature + body) требует cross-span edits и range-finder в source. V2 с Plan 104.6 (rename).
- **`[M-104.5-organize-imports]`** (P3, home **Plan 104.5**) — `source.organizeImports` action kind advertised but not yet implemented (no-op body); V2 после Plan 104.3/104.6 когда symbol index доступен для dead-import detection.

## Follow-up: Plan 104.6 (Rename + Format-on-save)

- **`[M-104.6-symbol-table-rename]`** (P3) — V1 rename uses regex word-boundary scan across all files; does not distinguish `foo` declared in different scopes. V2: expose `resolve_symbol_at(module, pos) -> Option<Symbol>` from `compiler-codegen` for per-position symbol resolution; use it to restrict rename to the exact declaration + its references only.
- **`[M-104.6-nova-fmt-stdin]`** (P3) — Current `format_document` writes to a temp file. If `nova fmt` adds `--stdin` support, switch to piped stdin to avoid I/O overhead.
- **`[M-104.6-ontypeformat-more-triggers]`** (P4) — Add `,` and `;` triggers for onTypeFormatting (auto-space after comma etc.).

## Follow-up: tree-sitter-nova (grammar sync)

| Маркер | Статус | Действие |
|---|---|---|
| `[M-104.7-v4-keywords]` | OPEN | Будущие keywords → grammar update при добавлении в lexer |
| `[M-104.7-query-update-priv]` | ✅ CLOSED 2026-06-17 | highlights.scm updated — priv/pub/extern highlighted |

## Follow-up: Plan 91.8b (remove @eq/@lt/@le/@gt/@ge)

| Маркер | Статус | Home | Действие |
|---|---|---|---|
| `[M-91.8b-remove-old-ops]` | ✅ CLOSED 2026-06-17. @eq/@lt/@le/@gt/@ge удалены из компилятора и std. | Plan 91.8b | ✅ done |

## Follow-up: Plan 91.14 (Debug protocol + format spec)

| Маркер | Статус | Действие |
|---|---|---|
| `[M-91.14-sum-debug-variants]` | OPEN | Sum-type debug V1 outputs type name; extend `synthesize_debug` for per-variant output |
| `[M-91.14-str-from-debug-walker]` | OPEN | `default_body_calls_satisfy_for` doesn't check `str.from_debug`; add check |
| `[M-91.14-format-dsl-extensions]` | CLOSED (D419, Plan 152.7.2) | `:hex`/`:.3`/`:pad-N` shipped in D258/152.7-B; per-type spec dispatch (`@display_fmt`) shipped in D419 |
| `[M-152.7.2-interp-direct-primitives]` | OPEN | Interpolation engine writes user-type Display/Debug directly into the sink already (Plan 175 Ф.3(d)); primitives (`nova_int_to_str` etc. in `emit_interpolated_str`) still allocate an intermediate `nova_str` before copying into `StringBuilder` — collapse to a direct-to-sink write |

## Follow-up: Plan 91.8c (generic array sort/min/max/_by)

| Маркер | Статус | Действие |
|---|---|---|
| `[M-91.8c-pdq-sort]` | ✅ **CLOSED (Plan 153.3.1, 2026-06-18)** | `@sort_unstable*` upgraded from heapsort to pdqsort. `std/sort.nv @sort_of` remains insertion sort (NovaArray path, separate blocker `[M-153.x-array-new-not-vec]`). |
| `[M-91.8c-int-min-max-dispatch]` | OPEN | Pre-existing CC-FAIL: `[]int @min()/@max()` resolve to `f64.min` (2-arg) in codegen. Needs dispatch fix in emit_c.rs method resolution. See plan91/sort_basic.nv. |
| `[M-91.8c-direct-index-method]` | ✅ **CLOSED 2026-06-17** | `@[i].method()` now dispatches correctly — `ExprKind::SelfAccess` arm added to `compute_array_elem_type_for_obj` (emit_c.rs ~14248); `emit_monomorphized_method` derives concrete element C type from recv_c and registers under `array_element_types["nova_self"]`. No intermediate binding needed. 5/5 tests PASS; 14/14 regression PASS. |

## [M-172.1-d174-sync-consume-registry] — guard-consume интеграция sync-типов не кредитуется (2026-07-02)

`consume g = mu.lock(); g.unlock()` → ложный `[D133-not-consumed]`: `LinearityRegistry`
строится из module.items (Plan 169.2 §4 — хардкод external_sources удалён), а external
sync-типы (`MutexGuard`/`ReadGuard`/`WriteGuard`/`Permit`/`OnceGuard`) приходят через
`load_builtins` → их consume-методы (`unlock`/`release`/`commit`/`abort`) реестру неизвестны →
обязательство никогда не закрывается. Pre-existing: `nova_tests/plan103_9/guard_cross_scope_transfer`
красный на baseline 69d64b7a (проверено temp-worktree 2026-07-02). Закрывается в U.1.3b
(sync Gap B + миграция load_builtins): consume-метаданные sync-типов должны прийти из
`.nv`-деклараций реестровым путём (§3 — никакого нового хардкода). Тест-драйвер готов:
`spec_tests/inprogress/d174_sync_consume_guards.nv` (перенести в conformance после фикса).

### [M-172.1-d174-sync-consume-registry] — ✅ РЕАЛИЗОВАН (2026-07-02, тот же день)

Закрыт срезом Gap-B-lite: (1) `builtin_sources()/builtin_modules()` — единый список
embedded .nv (кормит и load_builtins, и чекер); (2) Linearity/ConsumeRegistry
`absorb_external()` — consume-типы/методы/method_return_types из builtin-деклараций;
(3) sync.nv добавлен в `builtin_sig_modules()` (чекер знает Mutex/guards как типы +
method-сигнатуры); (4) infer_expr_type: static-method return для конкретных типов
(`Mutex.new()` → Mutex); (5) f1_check_call: `-> Self` субституируется в receiver-тип
ПЕРЕД материализацией в канал (раньше канал нёс Named{Self} → codegen травился
name-keyed `fn_ret_new` last-wins → чужой тип); (6) legacy extern-method return из
ExternalRegistry (реестр .nv-деклараций); (7) phase-safety: `recv_c_type_materialized`
для Ident-ресиверов в side-channel пробах (D166 defer-hoist, divergence-скан,
protocol-key, variadic, CancelToken, legacy Call-arm) — модульный namespace/pre-pass
не заходит в P67-пробы. d174 в conformance (36/36 PASS с plan103_9); регресс чист
(effects/basic, syntax/anonymous_embed — pre-existing на baseline 69d64b7a).

## [M-172.1-d174-with-lock-generic-ret] — ✅ РЕАЛИЗОВАН (2026-07-02, U.1.3b sync-inline)

`Mutex.with_lock[R](body fn() -> R) -> R` — method-level generic на builtin
Nova-body методе; return из closure-arg не инферится (D119-механика покрывает
только mono-receiver'ы `____`). Корпус plan103_9 with_lock не вызывает. Тест
thin-wrapper'а — после реализации (кандидат: U.1.3b sync-inline).

## [M-172.1-d174-once-try-start-option] — ✅ РЕАЛИЗОВАН (2026-07-02, U.1.3b sync-inline)

Nova-body `Once.try_start() -> Option[OnceGuard consume]` codegen'ом не эмитится
(builtin Nova-body метод, эмитился как struct-member-call → CC-FAIL). Корпус
использует extern-пару try_start_won()+make_guard(). Тест Option-формы — после
U.1.3b sync-inline.

## [M-172.1-d48-tagged-template-desugar] — ✅ РЕАЛИЗОВАН (2026-07-02, тот же день)

Диагноз (после расследования): не краш — НЕВЕРНЫЙ результат. Emit-arm
(emit_c.rs:22408) — «Bootstrap: tag function ignored» — тег-функция игнорируется,
шаблон склеивается как строка; в кейсе `d48_fmt`x=${x}`` даже `${}` не сплитится
парсером → эмитится СЫРОЙ литерал `"x=${d48_x}"`. D48-норма (вызов
`tag(parts []str, args []T)`) не реализована ни в одном слое. Реализация:
(1) parser — интерполяционный split внутри tagged-template (parts/args);
(2) emit — построение []str parts + []T args + вызов/моно tag-функции.
Тест-драйвер: spec_tests/inprogress/d48_tagged_template.nv.


## [M-172.1-sync-extern-narrowing-migration] — ✅ ЗАКРЫТ 2026-07-04 (гейт снят, корпус мигрирован — blast-radius 2 fn)

Merge sync-сигнатур в чекер (builtin_sig_modules) включил narrowing-enforcement на
sized-atomic API (i32/u8-параметры), а корпус atomics/sync (~150 файлов) писался
ДО enforcement'а (int-вары в sized-параметры). Гейт: E_IMPLICIT_NARROWING НЕ
эмитится для `callee.is_external` (оба сайта в types/mod.rs, помечены маркером).
Снять после плановой миграции корпуса (as-касты в тестах) — вместе с полной
U.1.3b миграцией sync на import.
**Носитель: Plan 172.1 FIN-фаза** (снятие гейта = часть удаления legacy-снабжения sync;
механика миграции — прецедент Plan 172.2 Ф.3: детект-режим → as-касты → включение).


## [M-172.1-extern-cname-dedup-overloads] — дедуп extern-деклов по c_name упрощён (2026-07-02) → ✅ ЗАКРЫТ Plan 174.6 M1 (2026-07-04)

При inline-merge builtin-модуля (import std.runtime.sync поверх builtin-снабжения)
дубликаты extern-деклараций схлопываются по `c_name` (external_registry merge).
Корректно для текущей runtime-схемы (1 c_name = 1 сигнатура), но при появлении
НАСТОЯЩИХ overload'ов с одинаковым c_name (разные param_c_types) дедуп молча
съест вторую сигнатуру → неверный резолв перегрузки. Ужесточить: ключ дедупа =
(c_name, param_c_types), конфликт = ошибка компилятора.
**ПОГЛОЩЁН Plan 174.6 (C-FFI ABI types).** M0 (spec, 2026-07-04) — amend D282 rule 2
(рекурсивный C-ABI тип-лист, params+return) + D353 (fn-ptr ABI-тег `*extern "C" fn`) +
D216 cross-amend. Ужесточение ключа дедупа `(c_name, param_c_types)` = чекер-часть
**Plan 174.6 M1** (вместе с `E_FFI_NON_C_ABI_TYPE`); сайт помечен комментарием в
external_registry merge.
**✅ ЗАКРЫТ Plan 174.6 M1 (2026-07-04):** ключ дедупа в `emit_c.rs` (user-external merge)
изменён `c_name` → `(c_name, param_c_types)`. True-duplicate (builtin-supply ↔ `import`
double-feed, тот же c_name + та же сигнатура) молча схлопывается (byte-identical). Genuine
overload-collision (один C-символ, РАЗНЫЕ `param_c_types`) → `E_FFI_C_NAME_OVERLOAD_CONFLICT`
(compile error) вместо тихого проглатывания второй сигнатуры. Zero-regression: atomics
(import std.runtime.sync — двойное снабжение) PASS 6/0 без конфликта.
**Adversarial-аудит M0 (2026-07-04, spec-doc-фиксы):** в D282 rule 2 добавлены `uint`
(=`nova_uint`, D130), C-ABI fn-ptr base-case (`*extern "C" fn` как параметр/поле —
закрыл D282↔D353 gap), `Option[RawPtr]` (было узко `Option[*T]`), исправлен пример
циклического value-record (`type Node value {…}` — было без `value` → self-contradiction
с heap-record negative-list); в D353 коэрция получила условие (3) **effect-free/total**
(soundness: captureless-fn с любым эффектом, не только `Fail`, звался бы из C без
handler-фрейма → unsound). **Error-index-долг (deferred → M1, debt-нота в D353 Scope):**
`E_FFI_NON_C_ABI_TYPE`/`E_CALLBACK_THROWS_OVER_C_ABI`/`E_CLOSURE_HAS_ENV` заносятся в
09-tooling error-index **вместе с чекером M1**, который их эмитит (message-text уже в
Plan 174.6 §4).

## [M-172.1-var-types-cu-name-leak] — var_types один namespace на CU, last-wins (2026-07-02)

`var_types` в codegen ключуется голым именем на весь CU: одноимённые локалы из
разных пир-файлов folder-модуля перезаписывают друг друга (last-wins) — тип локала
чужого файла протекает в твой (загадочный type-mismatch). Тактическая норма:
префиксация локалов в conformance-тестах (test-conventions.md, согласовано
2026-07-02). Правильный фикс: скоупить локалы per-fn/per-file (ключ = (fn_id, name)
или span). Родственно §21 d-status (user-shadow generic-типа протекает в чужой
модуль — архитектурная проблема резолвера).
**Носитель: Plan 172.1 FIN-фаза** (при удалении legacy var_types-путей).


## [M-172.1-lifted-legacy-arms] — поднятые legacy-армы в dispatcher (2026-07-04)

Финиш tally→0 выполнен ПЕРЕНОСОМ остаточных legacy-армов в dispatcher
(Channels 6i-6z: None/If/Index/Match/Member/RecordLit/Ident/Call/финальный
остаток). Это переходная форма: армы = дословные копии state-логики,
подлежат ЗАМЕЩЕНИЮ чекер-каналами (продолжение линии марафона: ~45%
исходного tally уже замещено настоящими каналами — TypeParam/mono-map/
expected-type/arg-binding/resolve-семейство). legacy_inner —
недостижимая заглушка; удаление мёртвого кода (wrapper+заглушка+прото-армы,
~3.5k строк) — следующий атом FIN.
**Носитель: Plan 172.1 FIN.**

**Update (Plan 172.12 A4, заход 8, 2026-07-07):** проверка кода показала — литерального
`legacy_inner`-wrapper'а БОЛЬШЕ НЕТ (последующие 172.1 FIN-заходы влили его тело НАПРЯМУЮ в
`infer_expr_c_type`, сейчас ОДНА функция ~7350 строк, Channels 1-6 → 6b-6z → generic
ExprKind-fallback). Упоминания `legacy_inner` в комментариях — stale (описывают состояние ДО
слияния). Нет «недостижимого кода после panic» для удаления — весь код внутри функции
ДОСТИЖИМ (реальная fallback-логика, не мёртвые дубли). A4 устранило ВСЕ raw
`Nova_`/`____`-decode операции внутри этих арм (греп-инвариант 0), переведя их в именованные
debt-точки — это ПРЕДПОСЫЛКА для будущего IR-лоуэринга (Ф.2-Ф.4 зонта 172.12), не само
замещение. Маркер остаётся ОТКРЫТЫМ.


## [IDEA-172-typed-ir-mono] — ✅ ОФОРМЛЕН ПЛАНОМ [172.12](172.12-typed-ir-mono.md) (2026-07-04)

Главный структурный разрыв с эталонами (rustc HIR/MIR, Zig ZIR/AIR, Swift SIL,
Go typed-AST+SSA): AST→C-текст напрямую, mono-identity на C-строках
(`Nova_Vec____nova_str*`), часть типовой правды в codegen-state. Typed-IR mono
убивает C-строковую identity, закрывает [M-172.1-lifted-legacy-arms] классом
и даёт базу constraint-инференсу. Рекомендация: НОВЫЙ под-план зонта 172
(следующий свободный номер; 172.6-172.11 исторически вынесены в 174 — не
переиспользовать), порядок: ПОСЛЕ 172.4 (value-ABI упростит representation).
Оценка: крупный (уровень 172.1).

## [IDEA-172-constraint-inference] — ✅ ОФОРМЛЕН ПЛАНОМ [172.13](172.13-constraint-inference.md) (2026-07-04)

Ad-hoc продюсеры канала (симптом марафона: каждый контекст — отдельный
продюсер) → унификационное ядро (Go types2/K2-класс) поверх готового канала.
Закрывает C6-полный (closures bidirectional), anon-RecordLit-expected,
flow-None классом. Порядок: после typed-IR (структурные типы повсюду).


## [IDEA-172-incremental-queries] — Инкрементальность/query-система (2026-07-04) — ДАЛЁКИЙ ГОРИЗОНТ

Rustc salsa-класс (мемоизация/инвалидация по запросам) / Zig incremental.
Для текущего размера компилятора и однопроходной скорости НЕ критично —
план не создаётся сознательно. Пересмотреть, когда (а) полный чек+кодоген
крупного проекта станет узким местом UX, (б) появится typed-IR (172.12) —
естественная граница мемоизации. НЕ носитель ближайших зонтов.


## [M-174.3-any-is] — `any`-тип + `is`/`try_as`-downcast: остатки (2026-07-04)

Plan 174.3 Ф.1+Ф.2 ВЫПОЛНЕНЫ (`any` = boxed `void*`→`NovaAny`, `v as any` явный+неявный
upcast, `x is T`/`try_as[T]`/flow-narrowing на type_id-реестре Plan 61, `[E_IS_NON_ANY]`).
Разблокирован Plan 173 Ф.4 (`Failure(any)`, `e is CancelError` строятся поверх). Остатки:

- **[M-174.3-any-as-fail-method]** — форма `x.as[T]?` (Fail-downcast, комплемент `try_as`).
  Парсер не принимает `.as` в member-позиции (`as` — ключевое слово); нужен либо
  контекстный allow `as` после `.`, либо переименование. `try_as[T]()` + narrowing
  покрывают извлечение — форма опциональна.
- **[M-174.3-match-pattern-is]** — `match { n is T => … }` / `is T =>` pattern-форма на
  `any` (D54 §«Pattern в match»). Реализованы операторная/`if`-форма + `try_as`; match-arm
  `is`-паттерн (binding + smart-cast) — отдельная работа в emit_match.
- **[M-174.3-heterogeneous-any]** — Ф.3: гетерогенные `[]any` (boxing элементов) +
  `Eq`/`Hash`/`Clone`/`Display`-thunks в `NovaTypeInfo` → `any` в `HashSet`/`HashMap`,
  сравнение/печать стёртых значений.
## [M-177-d77-codegen-4way-retract] — D77 4-way→2-way codegen (Plan 177 Ф.2b, отложено 2026-07-04)

D325 ретрактирует bare-throws auto-derive конверсий: из `try_from`(Result) компилятор
больше НЕ должен синтезировать bare `from`(throws) fallible-форму (остаются `from`
infallible + `try_from` Result — «4-way»→«2-way»). **Spec-часть уже внесена** (2026-07-01,
`08-runtime.md:1662`). Остался **codegen** — `from_targets`/`try_from_targets`-синтез в
`emit_c.rs` (регистрация ~4174-4224; использование в `.into()`/`.try_into()` codegen
~26467-26524). Декларации TryFrom/TryInto НЕ трогать. Отложено из Ф.2b: сложно/отдельно
(задевает всю conversion-auto-derive машинерию), separable от parse/read-триад. Требует
own regression-гейта. Домен Plan 177 Ф.3 или dedicated.

## [M-checker-unknown-method-stackoverflow] — чекер не отвергал неизвестный метод + `nova check` overflow (Plan 177 Ф.3, ✅ DONE 2026-07-04)

✅ **CLOSED (Plan 177 Ф.3, 2026-07-04).** Диагноз «рекурсия в method-resolution-
fallback» оказался **мисдиагнозом** — под капотом ДВА независимых бага:

**(1) §0/§1-дыра (первичный):** чекер НЕ отвергал вызов несуществующего метода на
**примитивном** ресивере (int/str/char/…). `is_primitive_recv_name` DEFER-гейт
(U.3.2/U.3.3) пропускал их — и вызов утекал в codegen: `int.nonexistent()` → **паника
`[P67-LEGACY]`** (`emit_c.rs:39841` «method call return type unknown — checker must
annotate»), `str.zzz()` → мис-тип `nova_int` → **CC-FAIL**. **Фикс** (`check_instance_overload`,
types/mod.rs): для REAL-примитива, если метод не резолвится НИ через `method_overloads`
(user/prelude/protocol), НИ через builtin-интринсики (новый `CEmitter::
primitive_instance_method_known` — единый источник имён D109/D74/f64/str/whitelist +
D73/D84 into/try_into), НИ через prefix-generic/blanket (`fn[T] T @m`) → чистая
**`[E_UNKNOWN_METHOD]`** (§6, с fix-hint). §7.3-калибровка: `never`/`any`/`unit`
ИСКЛЮЧЕНЫ (bottom/top/unit — `never`-ресивер = чекер не вывел конкретный тип, напр.
opaque net `TcpListener`); user/opaque/generic-ресиверы не тронуты (их набор в
method_table неполон). Blast-radius в detect-режиме: 464→0 ложных на nova_tests под
`nova test` (все остаточные под `nova check` — per-file cross-module/sibling-import
артефакты folder-модулей, резолвятся при реальной компиляции).

**(2) `nova check` overflow (вторичный, orthogonal):** worker-нити `cmd_check`
(main.rs) имели дефолтный **2 MiB** стек — `types::check_module` на prelude-merged
модуле переполнял его для ЛЮБОГО файла (даже `fn f()->int{5}` — проверено), НЕ только
unknown-method. **Фикс:** worker-стек → 64 MiB (как весь остальной компилятор —
`main.rs`/`test_runner.rs`). Теперь `nova check` завершается; unknown-метод = чистая
`[E_UNKNOWN_METHOD]`.

**Разблокировало** `spec_tests/conformance/neg/d325_*`: добавлен
`neg/d325_retracted_str_parse_method_neg.nv` (retracted `try_parse_int` →
`E_UNKNOWN_METHOD`), раннер НЕ крашит. Гейты: repro clean, conformance 39/39
(neg 38/38), baseline-delta 0 (strings/basics/generics/plan91/plan110/buffers/plan103_9
vs parent `714f0f43`), Rust build clean. Домен: чекер / Plan 177 Ф.3.

## [M-parse-int-overflow-returns-invaliddigit] — parse_int overflow → InvalidDigit (pre-existing, 2026-07-04)

`str.parse_int` на 20-значном числе (`"99999999999999999999"`) возвращает
`Err(InvalidDigit)` вместо `Err(Overflow)` (neg-фикстур `parse_int_overflow_err` RUN-FAIL).
**Pre-existing** — тело `@parse_int` byte-identical со старым `@try_parse_int`, тот же
результат на baseline `19b9c756` (НЕ регрессия D325-rename). Баг в арифметике overflow-
check `parse.nv` (либо signed-overflow UB до проверки, либо порядок check/accumulate).
Домен Plan 174.1 (primitive parse), вне D325-rename-scope Ф.2b.

## [M-172.1-opt-result-over-userenum-typedef-order] — Option[Result[int, UserEnum]] typedef-ordering (Plan 177 Ф.3, обнаружен 2026-07-04)

`sequence`/`partition` (prelude-коллекторы Ф.2c) над `[]Result[int, <UserEnum>]` (напр.
`ParseIntError`) → **CC-FAIL** `unknown type name 'NovaRes_nova_int_Nova_ParseIntError_p'`:
тело коллектора итерирует Vec (`for r in items`), итератор даёт
`Option[Result[int, ParseIntError]]`, и typedef Option-обёртки
(`NovaOpt_NovaRes_nova_int_Nova_ParseIntError_p_p`) эмитится ДО inner-Result typedef
(`NovaRes_nova_int_Nova_ParseIntError_p`), который под user-enum payload не регистрируется.
`Result[int, **str**]` (тот же коллектор) — **зелёный** (`err177_collectors` PASS): str-payload
Result регистрируется. Тот же класс VR-typedef-ordering, что чинили в Ф.2a
(`[M-177-result-over-named-tuple-codegen]`) / Ф.2c (`[M-172.1-U4-freefn-generic-return]`),
но для `Option[Result[T, UserEnum]]` из тела generic-коллектора. **Workaround в фикстуре**
(`d325_result_everywhere.nv` A4): parse_int → `Result[int, str]` через domain-str `match`-
канал. Домен Plan 172.1 (mono-typedef-registration). Вне scope Ф.3 (тесты/guards).

## [M-177-experimental-fallible-migration] — `std/_experimental` throw→Result (defer §9 Q3, Plan 177 Ф.4-аудит 2026-07-04)

Весь `std/_experimental/**` (**17 файлов**) ещё возвращает падающие операции через own-`Fail`
(throw), НЕ `Result[T,E]` — вне D325-конформности. **Отложено by-design** (Plan 177 §9 Q3):
`_experimental` = pre-prod поверхность, миграция под D325 едет с **стабилизацией каждого модуля**,
не в scope 177. Список: `encoding/{csv,hex,ini,toml,url}.nv`, `data/{semver,semver_range,sql}.nv`,
`crypto/{jwt,bcrypt}.nv`, `identifiers/{snowflake,ulid,uuid}.nv`, `math/statistics.nv`,
`text/regex.nv`, `time/cron.nv`, `concurrency/retry.nv`. **NB (R5):** `retry.@execute` /
`sql.in_transaction` несут `Fail[E]` **forwarded** из closure-параметра — легально даже после
стабилизации; мигрировать только intrinsic-ошибки (`Db`, retry own). Механика: throw→Err/return Err
+ Result-return-тип + call-sites `!!`/`?`/`.ok()` (тот же паттерн, что Ф.2a base64/json). Guard §8.2
исключает `_experimental` явно (`stable_std_files` retain). Консолидирует бывш. Plan 177 §6-список
(sql/jwt/snowflake/ulid/bcrypt/retry — был неполон). Home: per-module stabilization / Plan 177 §9 Q3.

**Обновление (волна промоушена std/_experimental → std, 2026-07-08):** `identifiers/snowflake.nv`,
`math/statistics.nv` **и `crypto/bcrypt.nv`** мигрированы throw→Result **и вынесены из
`_experimental`** (стабилизация + промоушен same-wave) — снять все три из списка. `bcrypt.nv`
попутно разблокировал и пофиксил свою транзитивную зависимость: `crypto/sha256.nv`
(array-repeat-литерал парсер-баг — см. `[M-sha256-array-repeat-literal-parser]`, ЗАКРЫТ) и
`encoding/hex.nv` (D133-not-consumed `buf.into()`→`buf.into_str()`, retired `str.len()`→
`byte_len()`, `with_capacity`→`.new()`+`.cap()`, throw→Result, ВСЕ теми же принципами D325) —
`sha256.nv`/`hex.nv` теперь ПОЛНОСТЬЮ зелёные (`nova test --full` PASS), но остаются в
`_experimental` (вне периметра 14-модульной волны, не промоутятся сейчас — только починены как
зависимость). Список остальных **14 файлов** без изменений (`sha256`/`hex` тоже сняты — они уже
не throw-based).

## [M-177-concurrency-throw-fallibility] — `std/concurrency` race2/with_timeout throw bare-str (Plan 177 Ф.4-аудит 2026-07-04; home Plan 173)

`std/concurrency/cancellation.nv` — `race2[T](a,b) -> T` (both-failed) и
`with_timeout[T](ms, body) -> T` (timeout) **`throw` bare `str`** через *inferred* Fail-эффект
(сигнатура написана `-> T` без литерала `Fail[` → conformance-guard §8.2, сканирующий `Fail[` в
сигнатуре, их **не ловит** — §14.3 плана 177). По D325 R1 timeout/both-failed = **expected
failure** → должны возвращать `Result[T, <Timeout/RaceError>]` (или `with_timeout` схлопнуть в
`within(...).ok()` — это throw-twin Option-формы `within`, ровно дуал, который D325 ретрактирует).
**НЕ конвертировано в Ф.4** (осознанно): structured-concurrency error-семантика = **Plan 173-домен**
(§10/§13 плана 177, вне scope 177) — нужен error-домен-тип + coordination с MultiError / 173 Ф.4
(typed errors) + смена 2 `#stable(since="0.1")` публичных сигнатур + sweep call-sites. Whole-subsystem
→ маркер, не тихий solo-fix (§7.7: не выдавать частичное за полное). **Home: Plan 173** (error-machinery
для concurrency). Смежно: усиление guard §8.2 до эффект-инференса закрыло бы blind-spot, но = компилятор-
в-тесте (дорого) — держим явный маркер вместо.
>
> **Резолюция Ф.4 #5 (2026-07-06): ОСТАЁТСЯ ОТКРЫТЫМ — обоснованно вне Ф.4.** Ф.4-домен = payload-типизация
> cleanup/outcome-поверхности (`ScopeOutcome.Failure(any)`, MultiError), НЕ throw-домен stdlib-concurrency-fn.
> `#5` (typed errors) даёт строительный блок, но полная конверсия race2/with_timeout всё ещё требует:
> (a) concurrency-error-домен-тип (`Timeout`/`RaceError`) — **Ф.3-остаток** (structured-concurrency семантика);
> (b) `with_timeout` **удаляется** per §3a п.4 (субсумирован `supervised(timeout:)`) → типизировать его throw =
> мёртвая работа, гейт **Plan 175**; (c) смена 2 публичных `#stable`-сигнатур + sweep call-sites. Держим маркер.
## Plan 178 Ф.1 — std/http message-model followups (2026-07-04)

Приземлён `std/http/` message-model + URL + валидаторы (см. simplifications.md, D358/D359). Отложено в Ф.2+ (маркеры, гейтнуто НЕ упрощено):
- **[M-178-body-http-effect-surface]** — `Http`-effect на потребляющих методах `Body` (park над транспортом при стрим-чтении); Ф.1 InMemory-путь I/O не делает → метод чист.
- **[M-178-body-transport-reader]** — transport-backed `BodyReader` (chunked/CL/h2-DATA декодер над сокетом, holds socket → станет `consume`); Ф.1 = in-memory pull.
- **[M-178-body-copy-json-trailers]** — `Body.@copy_to` (fs-gate 176) / `@json[T]` (serde-gate 180 Ф.4) / `@trailers` (Ф.2).
- **[M-178-body-text-charset]** — charset-aware `@text` (latin1-fallback по Content-Type) — Response-контекст Ф.2; Ф.1 = строгий UTF-8.
- **[M-178-bodyreader-option-eof-eq-ordering]** — план-форма `@next_chunk -> Result[Option[[]u8]]` (None=EOF) упирается в codegen forward-decl-ordering-баг eq `Option[Option[[]u8]]`; Ф.1 = `@at_eof()` + `Result[[]u8]`.
- ~~**[M-178-consume-field-ctor-from-var]**~~ ✅ **CLOSED 2026-07-13** (ветка m178-consume-field) — consume-analysis теперь распознаёт move голой owned-переменной/consume-параметра в consume-поле record-литерала (типизированного, анонимного-из-контекста, punning; sum record-варианты тоже) = потребление биндинга; use-after-move/двойной move ловятся (D131). Композиция с D188 v3: consume-поле литерала в tail/return re-consume блока = consume-позиция (дизарм при конструировании; codegen `collect_reconsume_occurrences_rec` расширен на RecordLit). Спека: D133 §«Что считается consume» (02-types.md) + D188 v3 п.2/п.3/п.4 (03-syntax.md). Тесты: `spec_tests/conformance/d133_consume_field_ctor_from_var.nv` + neg (use-after-move, double-move) + v3-хвост в `d188_reconsume_block.nv`. nova-tls: pass-through `tcp_move` удалён, `TlsStream.wrap` строит `{ tcp: stream, session }` напрямую (ветка m178).
- **[M-178-errsource-net/utf8/io/tls]** — payload-типизированные `ErrSource`-варианты добавляются при приземлении зависимостей (Ф.2 / 176 Ф.0.5 / 116); enum OPEN → non-breaking. **`Compress(CompressError)` ✅ приземлён 2026-07-06** (auto-decompress, разблокирован D381) — остаются `Net`/`Utf8`/`Io`/`Tls`.
- **[M-178-setcookie-expires-timestamp]** — typed `expires Option[Timestamp]` (IMF-fixdate→epoch, Plan 175); Ф.1 несёт `max_age`.
- **[M-178-message-builders]** — RequestBuilder/ResponseBuilder/verb one-shots/`error_for_status` — Ф.2.
- **[M-178-url-decode-canonical-from-bytes]** — `decode_query` UTF-8-валидация self-contained; при landing canonical `str.from_bytes`+`Utf8Error` (176 Ф.0.5) — делегировать + `ErrSource.Utf8`.

**Pre-existing compiler-баги (обнаружены при Ф.1, НЕ 178-specific — воспроизводятся на std.net):**
- `[P67-LEGACY] Enum.UnitVariant.method()` (напр. `NetError.Eof.to_str()`) и `x == Enum.Variant` (bare unit/payload-variant в `==` RHS) → ICE / mis-type в `nova_int` (infer_expr_c_type fallback). Обход: bind-first / `match`. Кандидат в checker-annotation gap.
- `!!` на `Result[(), E]` (unit-Ok) mis-infer'ит тип (HttpError* = nova_unit). Обход: `match`/`?`-in-fn.
- Индексация `[]T`-ПАРАМЕТРА `params[i]` теряет индекс в codegen (вызывает метод на всём Vec). Обход: `.get(i)`.

## Plan 178 Ф.2 — HTTP/1.1 client CORE followups (2026-07-04)

Приземлён plaintext HTTP/1.1 client-core (см. simplifications.md). Nested submodules `std.http.client` (client/wire/mock) + `std.http.transport` (real_http). Отложено/gated:
- **[M-178-client-live-pool]** — keep-alive DECISION-logic + pool-config есть; live socket-reuse отложен (нужен framed-read через kept-open socket + fix [M-net-payload-variant-static-lowering]). CORE = `Connection: close` + read-to-EOF.
- **[M-178-timeout-needs-173]** (PRIMITIVE LANDED 2026-07-06, Plan 174 / D408) — ключевой примитив из refined-gate приземлён: **`supervised(deadline: Monotonic)` / `supervised(timeout: Duration)`** — СТРУКТУРНАЯ конструкция (не fn-обёртка), тело выполняется с любым effect-row (`Http`/`Net`/`Fail[E]`), эффекты протекают → снимает effect-mismatch, который валил `within`/`with_timeout` (чистые `fn()->T`). Механика: таймер→областная отмена (путь `cancel:`)→типизированный `TimeoutError` (ловится `is TimeoutError`); USER-precedence; вложенность min-точкой; sleep прерывается рано. Тесты 8/8 (`std/concurrency/`). **ОСТАТОК для 178 Ф.2 (timeout-by-default В КЛИЕНТЕ):** обернуть `HttpClient.send()` в `supervised(timeout: cfg.timeout) { ... }` + прокинуть `HttpClientBuilder.@timeout(d)`; consume-`Response` через scope-boundary = linearity-hazard (проверить) → маркер `[M-178-client-timeout-wiring]`. Сам примитив 173/174 больше НЕ блокер.
- **[M-174-retract-with-timeout]** ✅ **CLOSED 2026-07-10** (Plan 174 / §3a п.4; гейт — Plan 175 Ф.3a мокабельный Monotonic + Ф.5d) — `within[T]`/`with_timeout[T]` УДАЛЕНЫ из `std/concurrency/cancellation.nv` (`race2` остаётся, не субсумирован). Call-сайты мигрированы: `nova_tests/concurrency/cancellation_test.nv` (4 within-теста удалены), `mn_closure_spawn_gcroot_test.nv` (последний тест → `race2`; заодно починен НЕЗАВИСИМЫЙ pre-existing mut-capture баг D415 §2 в helper'е `run_int`, найден при миграции), `examples/real_world/audit.nv` (иллюстративный → `supervised(timeout:)`). Опасение «cancellation.nv сломан str.len/ro-field дрейфом» не подтвердилось на момент закрытия (файл компилировался чисто изолированно). Home: Plan 173 Ф.3-остаток.
- **[M-174-parallel-for-deadline]** (Plan 174, OPEN 2026-07-06) — `parallel for` должен зеркалить `deadline:`/`timeout:`/`cancel:` параметры `supervised` (план 173 §3a п.3). `parallel for` десугарит в `supervised { for … spawn }`, но keyword-args у `ParallelFor` нет (парсер) → добавить. Наследование ambient-дедлайна во вложенные `parallel for` УЖЕ работает (через `nova_scope_init` inherit). Home: Plan 173.1 / 174.
- **[M-178-client-timeout-wiring]** (Plan 178 Ф.2, OPEN 2026-07-06) — timeout-by-default В HTTP-КЛИЕНТЕ: обернуть `HttpClient.send()` в `supervised(timeout: cfg.timeout) { … }` + `HttpClientBuilder.@timeout(d)`/`@no_timeout()`. Примитив (D408) готов; проверить consume-`Response`-через-scope-boundary linearity-hazard. Home: Plan 178.
- **[M-178-autodecompress-needs-179]** (✅ **LANDED 2026-07-06**) — auto-decompress приземлён в `std.http.client`. Клиент шлёт `Accept-Encoding: gzip, deflate` по умолчанию (opt-out `HttpClientBuilder.@no_decompress()`); `finalize_response` прозрачно декодит `Content-Encoding: gzip`/`x-gzip` (`gzip_decode`) и `deflate` (`zlib_decode`, fallback на raw `inflate` для не-zlib-сендеров), снимает `Content-Encoding` + переписывает `Content-Length` на декодированную длину. Bomb-guard: `max_decompressed` (default 64 MiB, D334; `@max_decompressed(n)`/`< 0`=без cap) прокинут как `max_output` → превышение = `Err(BodyTooLarge)`, НЕ OOM. Ошибки декода (corrupt/framing/checksum) → `HttpError{Protocol}` + типизированный `ErrSource.Compress(CompressError)` (via `HttpError.from_compress`). `br` (brotli) закрыт — нет кодека `[M-178-autodecompress-br]`. Круговой + neg(бомба) тесты: `nova_tests/http_decompress/decompress_test.nv` (gzip/deflate round-trip через mock-encode, opt-out, bomb→BodyTooLarge) PASS. Разблокировано фиксом **D381** (collision-aware mangling — compress+http `ErrorKind` co-present в одном CU; работает и на merge-base baseline, доказано). Добавлен `CompressError.@is_bomb()` (различает bomb без импорта OPEN `ErrorKind`-вариантов, чей `Other` коллидировал бы). См. `[M-sync-crossmodule-samename-type-collision]` (CLOSED).
- **[M-178-autodecompress-br]** — `br` (Brotli) auto-decode отложен: нет brotli-кодека в `std.encoding.compress` (только gzip/deflate/zlib/raw-inflate). Клиент НЕ рекламирует `br` в `Accept-Encoding`; если сервер всё же шлёт `Content-Encoding: br`, тело возвращается ENCODED (as-is, заголовок сохранён — вызыватель декодит сам). Landing = brotli-кодек в 179-следующем.
- **[M-178-typed-json-needs-180]** (CLOSED 2026-07-06) — типизированный decode приземлён как **`json_decode_body[T Deserialize](body []u8) -> Result[T, HttpError]`** (`std.http.serdejson`, поверх serde `T.deserialize`, DeError→`HttpError{Protocol}`+source). 4 pos/neg-теста (`nova_tests/http_typed/`, record-DTO round-trip + null-Option + malformed + missing-field). Два вынужденных design-решения (оба — codegen-обходы, не упрощения): (a) FREE-fn turbofish, НЕ `Response.@json_as[T]()` — generic **метод** с type-param только в return-позиции монеморфизируется в `nova_int` (turbofish игнорится, silent-miscompile) [M-codegen-method-return-turbofish]; (b) отдельный модуль `std.http.serdejson`, НЕ client.nv — serde в большом multi-file `nova_tests.http` CU роняет `NovaRes_..._SerError` forward-decl (protocol-vtable ordering) [M-codegen-serde-vtable-forwarddecl]. dynamic `@json()->JsonValue` — как было.
- **[M-178-autodecompress-needs-179]** (CODEGEN-UNBLOCKED 2026-07-06 via D381) — НЕ gated на Plan 179 (decode `gzip_decode`/`zlib_decode`/`inflate` ЕСТЬ). Был блокер: codegen манглил nominal-типы по КОРОТКОМУ имени → `compress.ErrorKind` и `http.ErrorKind` = ОДИН `struct Nova_ErrorKind`/`NOVA_TAG_ErrorKind_*` → `redefinition` при co-presence. **Снят D381** (collision-aware module-qualified mangling): http+compress `ErrorKind` co-present в одном CU теперь компилятся+линкуются (доказано conformance PASS 1/0 с d358+d333). Auto-decompress-в-client логика + `ErrSource.Compress(CompressError)` больше НЕ гейтнуты codegen'ом — осталось приземлить сам client-код (Ф.2). См. `[M-sync-crossmodule-samename-type-collision]` (CLOSED).
- **[M-178-typed-json-needs-180]** (CLOSED 2026-07-06) — типизированный decode приземлён как **`json_decode_body[T Deserialize](body []u8) -> Result[T, HttpError]`** (`std.http.serdejson`, поверх serde `T.deserialize`, DeError→`HttpError{Protocol}`+source). 4 pos/neg-теста (`nova_tests/http_typed/`, record-DTO round-trip + null-Option + malformed + missing-field). Два вынужденных design-решения (оба — codegen-обходы, не упрощения): (a) ~~FREE-fn turbofish, НЕ `Response.@json_as[T]()`~~ — **СНЯТО 2026-07-06:** `[M-codegen-method-return-turbofish]` CLOSED, метод-форма `Response consume @json_as[T Deserialize]()` теперь приземлена (free-fn остаётся substrate); (b) отдельный модуль `std.http.serdejson`, НЕ client.nv — serde в большом multi-file `nova_tests.http` CU роняет `NovaRes_..._SerError` forward-decl (protocol-vtable ordering) [M-codegen-serde-vtable-forwarddecl]. dynamic `@json()->JsonValue` — как было.
- ~~**[M-178-https-needs-116]**~~ **CLOSED 2026-07-10 (Plan 116 Ф.5.3):** `real_http()` ветка `secure=true` → `https_send_over_net` (TLS-слой поверх `TcpStream`: SNI=host, SystemRoots webpki-roots, ALPN http/1.1) вместо `Err(Tls)`. Код верифицирован в compress-free CU (mock non-TLS peer → `HttpError{Tls}` детерминированно); TLS-слой — std/tls loopback-тесты. **NB:** полный http-CU прогон (`client_test`/`real_test`) блокирован PRE-EXISTING дефектом `[M-compress-checksum-structvariant-ctor-xmodule]` (см. ниже) — не Plan-116-регрессия. ~~**Осталось (followups):** `ErrSource.Tls(TlsError)` типизированный source `[M-178-errsource-tls]`~~ **CLOSED (M-178, эта волна):** разгейчен ПОСЛЕ фикса mono-кэша конструкторов enum-вариантов в codegen (`debt_find_variant_ctx` / `infer_expr_c_type` Channel 6, compiler-codegen/src/codegen/emit_c.rs — bare `Net(e)` резолвился по первому/короче-имени кандидату среди коллидирующих `ErrSource.Net(NetError)`/`TlsError.Net(NetError)` без учёта call-сайт контекста; новый `expected_sum_hint`, распространяемый из declared field-типа через `emit_record_field_value`). `HttpError.from_tls(kind, TlsError)` в error.nv; real.nv использует его на обоих site'ах (handshake + write/read). Остаётся: кастомные корни/self-signed через HttpClient `[M-116-https-client-custom-roots]` (SystemRoots хардкожен в real_http; нужен client-TLS-config хук для self-signed loopback HTTPS-интеграции).
- **[M-compress-checksum-structvariant-ctor-xmodule]** ✅ **CLOSED 2026-07-10** (найдено Plan 116 Ф.5.3, зафиксирован Plan 173 P1-волной) — `std/encoding/compress/error.nv:121` `{ kind: Checksum { kind, expected, got }, … }` (**struct-payload** variant-конструктор `ErrorKind.Checksum`) → `[E_UNKNOWN_TYPE] unknown type Checksum` в БОЛЬШОМ multi-module CU (http: `client_test`/`real_test`), где co-present несколько `ErrorKind` (io/http/net/compress). Компресс-модуль СОЛО компилится (checksum_test PASS); ломался только в http-CU.
  **Root cause (НЕ codegen, ЧЕКЕР):** E_UNKNOWN_TYPE-гейт для RecordLit (`types/mod.rs`
  `TypeCheckCtx::walk_expr`, Plan 173 Ф.5 zero-tolerance) искал имя bare-варианта
  (`Checksum`) через `self.types.values().any(|td| matches!(&td.kind, Sum(vs) if
  vs.iter().any(|v| v.name == last)))` — но `self.types: HashMap<String, &TypeDecl>`
  keyed ПО ИМЕНИ СУММЫ (`ErrorKind`); при co-presence НЕСКОЛЬКИХ одноимённых сумм из
  РАЗНЫХ модулей (`ErrorKind` объявлен в http/io/compress) `types.insert` перезаписывает
  запись — выживает только ПОСЛЕДНИЙ вставленный, варианты остальных (включая
  `Checksum`) пропадают из `types.values()`. Класс: собственная коллизия чекера,
  НЕЗАВИСИМАЯ от codegen-мангла D381 (тот уже collision-aware на стороне emit_c.rs;
  этот чекерный гейт — новый, добавлен ПОСЛЕ D381, коллизию не учитывал). f2e24febd
  чинил ДРУГОЙ баг (codegen call-routing tuple/unit-вариантов) — не тот же root cause,
  просто соседний класс cross-module sum collision.
  **Фикс:** `TypeCheckCtx` получил параллельное поле `sum_variant_names: HashSet<String>`,
  заполняемое ЛОССЛЕСС (прямой `Vec`-обход `module.items`/builtin-модулей — тот же loop,
  что строит `types`, но НЕ подвержен overwrite, т.к. HashSet вставка не теряет записи
  других одноимённых сумм). E_UNKNOWN_TYPE-гейт теперь проверяет
  `self.sum_variant_names.contains(last)` вместо `self.types.values().any(...)`.
  **Регресс:** `nova_tests/xmodule_struct_variant_ctor_test.nv` (+ 2 co-equal модуля
  `xmodule_struct_variant_ctor_a`/`_b`, каждый объявляет ОДНОИМЁННУЮ сумму `Kind` —
  один со struct-payload вариантом, другой без — зеркало реальной http-коллизии,
  изолировано от std). **Гейты:** `std/http/client/client_test` PASS (Checksum-ошибка
  ушла); `std/encoding/compress/*` (d333/d334/d335/d336) delta-0 PASS соло; conformance
  91/0; `nova check std` УЛУЧШЕН (baseline 2be6d7064: PASS 118/FAIL 32 →
  ветка: PASS 125/FAIL 25 — остаток FAIL это ДРУГОЙ pre-existing класс,
  `std/tls/cert_modes_test.nv` undefined-identifier хелперов + intentional neg-тест,
  не наш маркер). `real_test` отдельно падает на ДРУГОМ pre-existing дефекте
  (`std/tls/handshake_test.nv`: undefined identifier `panic` в multi-file-module CU) —
  вне scope этого маркера. Отдельно (не в scope): `gzip.nv` соло → CC-FAIL
  `incomplete type NovaValue_Deflater` (другой codegen-гэп того же модуля, честно
  зафиксирован как отдельный gap при первой эскалации, не тронут).

- **[M-tls-handshake-test-panic-undefined-multifile]** (обнаружено 2026-07-10 при
  гейтах Plan 173 P1-волны, честно зафиксировано — НЕ тронуто, вне scope) —
  `std/http/transport/real_test.nv` (транзитивно тянет `std.tls` для интеграционного
  TLS-сценария) CODEGEN-FAIL: `std/tls/handshake_test.nv:23/30/37` — `undefined
  identifier 'panic'`. `handshake_test.nv` компилится СОЛО чисто (PASS) — ломается
  ТОЛЬКО когда становится частью БОЛЬШОГО multi-file-module CU через `real_test`.
  Похоже на класс `[M-codegen-multifile-module-impl-synth]`/`[M-178-conformance-d357-
  d360-forwarddecl-bug]` (multi-file-module CU теряет что-то видимое соло) — не
  расследовано вглубь (root cause не найден, вне assignment этой волны). Гейт этой
  волны затронут не был: `[M-compress-checksum-structvariant-ctor-xmodule]` (выше)
  подтверждён закрытым независимо через `client_test` (PASS) — `real_test` падает
  на ЭТОМ, ДРУГОМ дефекте, не на Checksum.

- **[M-tls-cert-modes-test-undefined-helpers]** — ✅ **ЗАКРЫТ (2026-07-11, ветка
  `cert-modes-helpers`)**. Root cause найден: НЕ баг test-CU-резолва (эта модель —
  Plan 81 Ф.10 / Plan 169.1 Ф.8 sibling-merge — работала корректно и уже давала
  `nova test std/tls` зелёным, 29/29, ДО этой правки). Баг был в `nova check`
  конкретно — `check_one_file` (`nova-cli/src/main.rs`) резолвил импорты через
  `resolve_imports_inline` (`include_test_peers=false`, build-режим), тогда как
  `walk_nv` (test_runner.rs) для folder-модуля С test-блоками уже схлопывает всю
  папку в ОДИН представительный entry (часто сам `*_test.nv`-пир, напр.
  `cert_modes_test.nv` для `std/tls` — первый по алфавиту). Build-режим фильтрует
  `_test.nv`-пиры этого entry → `handshake_test.nv` (хелперы `fixture_cert`/
  `fixture_key`/`must_tcp`/`must_listener`/`must_tls`) не мержился →
  undefined identifier. Соседние `cmd_check_explain_cache`/`cmd_check_telemetry_cache`
  в том же файле УЖЕ звали `resolve_imports_inline_ex(.., true)` с комментарием
  «mirror test_runner pipeline» — `check_one_file` был единственным нарушителем
  паритета. **Фикс:** `include_test_peers=true` в `check_one_file` + попутно
  добавлен sig-table pre-pass (`collect_all_signatures` →
  `check_module_with_sig_table`, зеркалируя `codegen_to_c`) — без него слияние
  большего числа test-пиров вскрыло ДРУГОЙ pre-existing паритетный гэп
  (`std/net/addr.nv`: локальная `ro io = NetError.IoError(..)` мис-резолвилась как
  вызов модуля `io` без sig-table-канала — тот же класс, что
  `[M-per-file-check-no-prelude-protocol-scope]`). **Гейты:** `nova test std/tls`
  PASS 1/1 (29/29 индивидуальных тестов, хелперы резолвятся); `spec_tests`
  (conformance) PASS 3/3 — δ0 против до-фикса; `nova check std/` — 125 PASS/23 FAIL
  (было 123/25) — **строго улучшение, 0 новых FAIL**: `cert_modes_test.nv` ушёл
  из FAIL, и БОНУСОМ `std/http/transport/real.nv` тоже (тот же check_one_file-баг,
  независимо от `[M-tls-handshake-test-panic-undefined-multifile]`, который остаётся
  открытым — под `nova test` `real_test.nv` теперь падает на ДРУГОМ, codegen CC-FAIL
  `NovaOpt_Nova_ErrSource_p`/`NovaOpt_Nova_TlsError_p` несовпадении, вне scope этой
  правки). Изменение изолировано в `nova-cli/src/main.rs::check_one_file` (только
  `nova check`) — `nova test`/`nova build` пайплайн (`codegen_to_c`) не тронут.

- **[M-per-file-check-no-prelude-protocol-scope]** — ✅ **ЗАКРЫТ попутно (2026-07-11,
  ветка `cert-modes-helpers`, побочный эффект фикса `[M-tls-cert-modes-test-undefined-
  helpers]` выше)**. Root cause совпал: `check_one_file` не строил sig-table pre-pass
  (`collect_all_signatures`/`check_module_with_sig_table`), которым `codegen_to_c`
  (test_runner.rs) уже пользовался — per-file `nova check` терял cross-module
  сигнатуры, отсюда ложные E_BOUND_UNKNOWN/E_IMPL_UNKNOWN_PROTOCOL/E_READONLY_CONTENT
  на файлах вне полного CU. Проверено ТОЧНО по исходному репро: `nova check
  std/runtime/string/chars.nv std/runtime/string/core.nv` — было FAIL, теперь PASS/PASS
  (0 FAIL); `std/collections/vec/{mutate,iter,access}.nv` — все три PASS/PASS/PASS.
  Родственный `[M-runtime-folder-run-ice-vec-ident]` (`nova test --full std/runtime`
  ICE) — ДРУГОЙ симптом (folder-run agregation, emit_c.rs ICE), НЕ затронут этой
  правкой, остаётся открытым.
- **[M-116-https-client-custom-roots]** (Plan 116 Ф.5.3) — HttpClient не даёт задать TLS-корни/self-signed; `real_http`/`https_send_over_net` хардкодит `ClientConfig.new(host)`=SystemRoots. `Http`-эффект `send(host,port,secure,request)` не несёт TLS-config. Нужен: client-builder TLS-хук (roots/InsecureSkipVerify/client-cert) → проброс через `Http`-эффект. Разблокирует self-signed loopback HTTPS-интеграционный тест через публичный HttpClient. За CORE Ф.5.3.
- **[M-178-client-policy-surface]** — Proxy/CONNECT-tunnel, SSRF-guard, cookie-jar, idempotent-retry+pool-eviction, 1xx-interim loop, NO_PROXY-матрица, TE:trailers, Expect:100 — за CORE.

## Plan 178 Ф.3 — HTTP/1.1 server CORE (2026-07-06)

Substrate-ASSESS: **UNBLOCKED** — `supervised`/`spawn`/`CancelToken`/`Semaphore`/`TcpListener.accept` все в main; net echo-тест (plan91_12) доказывает accept-loop→per-conn spawn-fiber паттерн. Приземлён **pure server-core** `std.http.server` (server/wire): `ServerRequest`/`ServerResponse`, `Handler` (concrete closure-newtype = Go-`HandlerFunc`, план Q27-fallback — избегает existential-boxing в Vec), `ServeMux` (Go-1.22 `{param}` + method + 405-`Allow` + 404 + HEAD→GET), `parse_request` (Host-mandatory + `..`-reject + CL-framing) / `serialize_response`, `serve_once` driver. **9 mock-тестов (no sockets) PASS** (`nova_tests/http_server/`): GET/param/POST-echo/404/405-Allow/HEAD/missing-Host-400/traversal-400/garbage-400.

Live-socket runner `std.http.servernet.handle_connection` (read framed request → `serve_once` → write → close). **LINK-препятствие СНЯТО 2026-07-06** (`[M-codegen-cross-module-ctor-emission]` FIXED): net+http co-present теперь компилятся И ЛИНКУЮТСЯ — servernet_smoke_test.c эмитит `nova_make_NetError_IoError` (0 ссылок на undefined `Nova_NetError_static_IoError`; baseline эмитил 1). Восстановлен смоук `nova_tests/http_servernet/servernet_smoke_test.nv` (loopback GET /health round-trip через `handle_connection`). **RUNTIME-блок (НЕ линкер):** живой сокет-путь падает `SIGSEGV` — но это **pre-existing net-substrate баг**, воспроизводится ДЕТЕРМИНИРОВАННО (5/5) на merge-base baseline И на чистом net-тесте (две fibers TcpListener/TcpStream, ZERO http, ZERO codegen-change). Symbol: не линкер — сегфолт в net-runtime (libuv/fiber). `[M-178-servernet-live-net-substrate-segfault]`. Смоук хранится (как plan83_12 live-socket соседи, тоже падающие в этом окружении: ICE `[P67-LEGACY] method=bind`/segfault) — НЕ в быстрой regress-выборке; зазеленеет с фиксом net-runtime. Серверная ЛОГИКА полностью покрыта mock'ами (`nova_tests/http_server`, 9 PASS).
- ✅ **[M-178-servernet-live-net-substrate-segfault]** (✅ ЗАКРЫТ Планом 183 (2026-07-07); real-корень — отсутствующий handler-install, НЕ M:N-гонка; P2, pre-existing net-runtime) — живой loopback сокет (supervised{spawn+spawn} над TcpListener/TcpStream) детерминированно сегфолтит (~100ms, 5/5) на baseline И current, на чистом net (без http). Также plan83_12 net-тесты ICE `[P67-LEGACY] Path call return type unknown for method=bind` (direct `.bind().is_ok()`-паттерн) и `.unwrap()` на `Result[_,NetError]` эмитит `Nova_Fail_fail(NetError*)` vs `nova_str`-сигнатура. Net live-socket substrate — переработан Планом 183 ([M-183-net-rework], 2026-07-06/2026-07-07). See also: [M-net-close-teardown-hang] (отдельный дефект).

- ~~**[M-net-close-teardown-hang]**~~ ✅ **RESOLVED 2026-07-11 (Plan 116 п.3a; ветка teardown-hang-close). ГЛАВНЫЙ ВЫВОД: наблюдавшийся `TIMEOUT` — НЕ runtime-teardown-hang, а COMPILE-фаза (медленная сборка тяжёлого CU под фоновой нагрузкой среды). Плюс попутно устранён реальный НЕЗАВИСИМЫЙ сокет-fd лик close-пути (defensive).** Разведка подозревала: `handle_connection_smoke` иногда ЗАВИСАЕТ в teardown ПОСЛЕ печати `PASS` (cross-thread close мигрировавших волокон через `nova_loop_defer_close`). **Расследование (плейбук §1-4) ОПРОВЕРГЛО гипотезу teardown-hang:**
  - **Runtime здоров:** прямой прогон собранного `.exe` = **0 hang / 924 прогона** (соло, ×300 подряд, 6 волн × 24 параллельно, ×150 под насыщением всех 16 ядер, dev+release C-codegen; +80 с anonymous-pipe stdout эмуляцией test-runner). Run-фаза = ~116ms всегда.
  - **`TIMEOUT` воспроизводится ТОЛЬКО через `nova test`** (не через прямой exe). Точечная диагностика (различающий маркер в двух Timeout-ветках `test_runner.rs` `run_with_timeout`: compile @2665 vs run @2769) дала **`[tt-diag] TIMEOUT in COMPILE phase after 56.9s`** — зависает НЕ exe, а `build_command` (компиляция C). Снимок процессов на 15-й секунде: 0 clang / 0 exe — компиляция .c→.o прошла, но полный build (тяжёлый CU: net+http+server+весь std prelude, ~16 рантайм-.c + тест) под текущей аномальной фоновой нагрузкой (`nova-lsp-v14` + пул VS Code, CPU baseline ~76%) не укладывался в **`EXPECT_TIMEOUT_MS 30000`** (лимит применяется И к compile, И к run).
  - **Контроль:** тот же тест с поднятым до `180000` лимитом → **5/5 PASS**, compile 21-46с (медленно, но успевает), run мгновенный. Под нормальной нагрузкой compile ~10с < 30с → PASS штатно. **Т.е. `TIMEOUT` = временный артефакт нагрузки среды, не дефект кода.**
  - **Попутно (аудит close-пути) найден и починен РЕАЛЬНЫЙ, но НЕ связанный с TIMEOUT дефект:** `nova_runtime_shutdown` (`compiler-codegen/nova_rt/runtime.c` per-worker cleanup) звал `uv_loop_close(&w->loop)` **ДО** финального `nova_loop_drain_closes`/`nova_loop_drain_calls`. Отложенный close/call-джоб (net.c `.close()` через `nova_loop_defer_close`; cross-thread issue через `nova_loop_defer_call`, Plan 183) при взводе `w->stop` в узком окне между enqueue и следующей итерацией воркера остаётся в очереди на момент join; прежний drain-ПОСЛЕ-close звал `close_cb` слишком поздно (`uv_run` для loop уже никто не крутит) → живой `uv_tcp_t`/`uv_udp_t` (сокет-fd) утекал НЕЗАКРЫТЫМ. **Fix (`7bd766963`):** дренаж call+close очередей + bounded-прокачка loop (≤64 тика, `uv_run` NOWAIT) ДО `uv_loop_close`, пока loop открыт и воркер joined (single-thread, гонки нет). Defensive, bounded overhead (пустые очереди = 0-1 тик). **Это устраняет лик, а НЕ наблюдавшийся TIMEOUT** (тот — compile-фаза).
  - **Гейты:** cargo build чисто; conformance 91/0; err173 28/0; std/net+std/http keystone (единственный FAIL — pre-existing `[M-tls-handshake-test-panic-undefined-multifile]`, не связан); репро через `nova test` PASS при штатной нагрузке / TIMEOUT только под тяжёлым фоном (compile-фаза).
  - **Рекомендация владельцу (не сделано в этой ветке — тюнинг-решение за владельцем):** compile-фаза этого CU близка к 30с-лимиту на нагруженной машине; при повторной флаке — поднять `EXPECT_TIMEOUT_MS` (тест несёт live-socket + самый тяжёлый co-present CU) либо разнести compile/run-лимиты в раннере. Диагностический флаг `NOVA_DEBUG_TIMEOUT_DUMP=1` печатает captured stdout/stderr для RUN-timeout — при будущем ПОДОЗРЕНИИ на runtime-hang использовать его: пустой dump ⇒ compile-timeout, непустой ⇒ настоящий runtime-hang.

Server-followups (за CORE, честные маркеры):
- ~~**[M-178-server-streaming]**~~ ✅ **ЗАКРЫТ (force-impl, Plan 116 worktree, 2026-07-12)** — streaming
  RESPONSE bodies (`Transfer-Encoding: chunked`/SSE) реализованы: `StreamBody`/`stream_body`
  (pull-source chunk producer) + `ServerResponse.stream`/`.sse` (server.nv);
  `serialize_response_head`/`encode_chunk`/`encode_chunk_end` (wire.nv, RFC 9112 §7.1, парный
  write-side к уже существующему клиентскому `decode_chunked`); `std.http.servernet.handle_connection`
  пишет каждый chunk отдельным `write_all` — ЖИВАЯ инкрементальная доставка. Write-backpressure
  (park при полном socket-buffer) — НЕ новый механизм: это уже существующее поведение `Net.write`
  внутри `@write_all` (std/net/tcp.nv), новый примитив не потребовался. `serve_once` (pure,
  mock-testable) полностью дренирует producer, так что wire-формат покрыт без сокетов
  (std/http/server/streaming_test.nv, 7 тестов) + live-socket smoke
  (std/http/servernet/rt/streaming_smoke.nv, EXPECT_TIMEOUT_MS). Разблокирует Plan 187 Ф.1
  (SSE-визуализация). Streaming REQUEST bodies (не запрошены гейтом 187) остаются вне объёма —
  см. `[M-178-server-policy-surface]` ниже (chunked-request decode).
  **По ходу вскрыты и точечно починены 2 доводки** (обе в std/http .nv, не в компиляторе):
  `StreamBody` (`value`-тип с closure-полем) была объявлена ПОСЛЕ `ServerResponse`, которая
  embed-ит её BY VALUE через `Option[StreamBody]` — C-структура требует complete type в месте
  embedding; переставлена перед `ServerResponse` (см. коммент-разметку в server.nv). И D131
  false-positive «использование, возможно, потреблённой переменной» в `servernet.nv`
  `write_streaming`: non-tail `match` со consume+`return` в одной ветке ядовит для ВСЕХ
  последующих statements (checker не увязывает `return`-terminated ветку с недостижимостью
  «после»); разбито на `write_streaming`/`write_stream_chunks` так, чтобы фоллибл match был
  единственным tail-expression.
  **Отдельно найден (НЕ ЗДЕСЬ почин, вне зоны std/http) реальный компиляторный дефект** —
  см. `[M-channel-generic-elem-type]` ниже.
- **[M-178-server-policy-surface]** — middleware onion, 100-continue, keep-alive, chunked-request decode, trailing-slash-301.
- **[M-178-server-graceful-deadline]** (PRIMITIVE LANDED+HARDENED 2026-07-06/2026-07-12-13, Plan 174 D408 + Plan 173 Ф.3) —
  `supervised(deadline:)`/`supervised(timeout:)` больше НЕ блокер: примитив в main
  с 2026-07-06 (D408); при исполнении 173 Ф.3 найден и починен реальный дефект —
  спавненный child, запаркованный на `Time.sleep`, будился РАНО отменой области
  (`nova_scope_deliver_cancel`), но не re-check'ал `cancel_requested` после
  `park_until`/на pre-arm fast-path'ах → «успешно» досыпал и докручивал тело до
  конца вместо unwind (leak: outer `supervised` уже вернул/бросил `TimeoutError`
  вовремя — это НЕЗАВИСИМый gate в `nova_supervised_run_impl` — а ребёнок ещё жил
  в фоне). Чинено в `fibers.h` (`_nova_sleep_via_libuv`/`_nova_sleep_via_driver`,
  4 сайта: 2×pre-arm early-exit + 2×post-park, shield-aware
  `nova_cancel_mask_load`+`nova_throw_cancel_reason`, паритет с
  `nova_fiber_yield`/channels.h/net.c). Regression: `std/concurrency/
  supervised_deadline_test.nv` test 8 (8/8 PASS). Спека НЕ менялась — D408 §3
  УЖЕ обещал «Sleep/сетевой park прерывается РАНО» нормативно; это conformance-
  фикс, не language-change. **Остаток (НЕ в этой волне, std/http-зона):**
  `servernet.nv` не имеет reusable multi-connection accept-LOOP вообще (только
  `handle_connection` + smoke-тесты) — сама проводка bounded-deadline-drain в
  такой цикл ещё предстоит написать; cancel-based stop-accept (без deadline)
  доступен уже сейчас на том же примитиве.
- **[M-channel-generic-elem-type]** (P1-ish, найден 2026-07-12 при работе над `[M-178-server-streaming]`,
  isolated-repro подтверждён вне std/http — pre-existing, компиляторный, НЕ мой зона в этой волне):
  `docs/channels.md` §«element type T is inferred from the first send/recv» **не выполняется** для
  non-`int` T. Repro: `ro { tx, rx } = Channel.new(4); tx.send("first")` (str) → emit_c.rs типизирует
  канал-элемент как `nova_int` НЕЗАВИСИМО от первого `send`, эмитит
  `nova_chan_writer_send(tx, (nova_int)(_nova_strlit_...))` (строка→int cast) → CC-FAIL (`nova_str` не
  влезает в `nova_int` param, компилятор C ловит). С `[]u8`-пейлоадом та же мистипизация КОМПИЛИРУЕТСЯ
  (указатель→int неявный C-cast разрешён) — silent semantics bug, не поймано компилятором. Документированный
  turbofish-эскейп `Channel[str].new(n)` (channels.md:124) **ICE**: `internal error at emit_c.rs:49360:
  [P67-LEGACY] Ident `Channel` not in var_types / not a sum-variant` — воспроизведено МИНИМАЛЬНО (голый
  `module chan_repro { test {...} }` вне std/, без деструктуризации тоже падает — не про
  `ro {tx,rx}`-паттерн). `int`-payload работает штатно (baseline sanity PASS). Обход в
  `std/http/server/streaming_test.nv` (см. коммент там): канал несёт `int`-индексы вместо
  `[]u8`/`str`-пейлоада напрямую. Нужен владельцу компилятора: (1) реальная T-inference от
  first-send/recv (docs claim), ИЛИ (2) минимум — turbofish `Channel[T].new` без ICE как рабочий
  escape hatch, ИЛИ (3) поймать non-int-payload-через-int-канал как ЯВНУЮ typed-error вместо silent
  pointer/int reinterpret для pointer-sized T (в компиляторе, не здесь).
- ~~**[M-channel-generic-elem-type]**~~ ✅ **(2)+(3) ЗАКРЫТЫ 2026-07-12** (ветка `channel-elem`,
  worktree `nova-capmig`, коммиты `<см. HEAD channel-elem>`). Найден 2026-07-12 при работе над
  `[M-178-server-streaming]`; репро было: `docs/channels.md` §«element type T is inferred from the
  first send/recv» не выполнялось для non-`int` T (`tx.send("first")` → CC-FAIL сырым C-компилятором;
  `[]u8`-пейлоад мистипизировался silently через pointer→int implicit C-cast; документированный
  turbofish-эскейп `Channel[str].new(n)` — ICE `[P67-LEGACY] Ident 'Channel' not in var_types`).
  Из трёх вариантов почина реализованы **два**:
  - **(a) честный gate** (`channel_payload_c_type_ok`, `emit_c.rs`): `.send`/`.try_send` теперь
    отвергают компиляцией (`E_CHANNEL_UNSOUND_ELEM_TYPE`) любой payload, чей C-тип не влезает
    без потерь в word-sized слот рантайма (`nova_str`, `nova_f32`/`nova_f64`, tuples,
    value-records) — вместо сырого C-cast-краша или silent pointer/int reinterpret. Word-safe
    типы (`int`, `bool`, `char`, fixed-width ints, любой pointer-sized `T` — `[]T`, records,
    `HashMap`, суммы) продолжают компилироваться как раньше (backward-compat подтверждена).
  - **(b) turbofish-ICE фикс**: `Channel[T].new(cap)` (`emit_call` + оба `infer_expr_c_type`-сайта)
    больше не падает — `Channel` как turbofish-база (`ExprKind::TurboFish{base: Ident("Channel")}`)
    распознаётся explicitly и эмитится идентично bare/Path-формам (`T` стёрт на рантайм-уровне
    регардless — `Nova_ChannelPair` нежанрик). `Channel[int]`/`Channel[bool]`/`Channel[str]`
    (последний — до честного (a)-gate на `.send`) больше не ICE.
  - **(1) реальная end-to-end T-inference НЕ реализована** — `rx.recv()` остаётся `Option[int]`
    на уровне codegen C-типа независимо от отправленного T (`infer_call_ret_c`,
    `"recv" | "try_recv" => "NovaOpt_nova_int"`); попытка реально ПОТРЕБИТЬ полученный `[]u8` как
    `[]u8` (`.len()`, индексация) даёт отдельный, другой ICE (`Index element type unknown for
    obj_ty="nova_int"`, `emit_c.rs:49468`) — тот же класс, что и (a)/(b), но НЕ починен этой волной
    (зона `resolve_return_channel`/`f1_check_call`, ~46293-48883, была явно вне периметра правки).
    Вынесено в отдельный **`[M-channel-real-elem-type-inference]`** (P2, требует полноценной
    generic-моно-типизации `Channel[T]` в checker+codegen — заметно больший объём, чем (a)/(b)).
  - **docs/channels.md**/**channels.ru.md**: убран misleading-пример `Channel[str].new(8)` (заменён
    на `Channel[int].new(8)`), добавлен явный §«word-safe T only» с перечислением supported/rejected
    и ссылкой на `E_CHANNEL_UNSOUND_ELEM_TYPE`.
  - **Регресс-покрытие**: `spec_tests/conformance/channel_elem_type_word_safe.nv` (int/bool/[]u8
    send, try_send, оба turbofish-варианта — pos) + `spec_tests/conformance/neg/
    channel_elem_str_payload_neg.nv` + `neg/channel_elem_turbofish_str_payload_neg.nv` (str payload
    через bare И turbofish `Channel[str].new` — оба ловят `E_CHANNEL_UNSOUND_ELEM_TYPE`).
  - **Гейты**: `cargo build --release` (nova-cli) чист (только pre-existing warnings). Conformance
    (`spec_tests/conformance`, один CU, `--positive --compile-error --timeout 300 --jobs 4`) —
    **97 PASS / 0 FAIL** (95 baseline + 2 новых neg). `std/http` (`--full --jobs 4`) — 9 PASS / 0 FAIL
    (два `servernet/rt/*smoke*` тайм-аутили ТОЛЬКО под `--jobs 4`-нагрузкой — компайл-фаза congestion,
    прецедент `[M-net-close-teardown-hang]`; PASS 2/2 при `--jobs 1 --timeout 180`). Точечный регресс
    по всем существующим Channel-юзерам (`nova_tests/err173_2`, `err173_3`, `negative_capability`,
    `plan83_10/neg`, `plan83_7`, `expected_runtime`, `std/concurrency`) — все зелёные, включая
    `err173_3/share_capture_ok_test` (`.try_send(j.payload)`/`.try_send(h.payload)`, int payload)
    и `std/concurrency/cancellation.nv` `race2[T]` (generic try_send, без каких-либо callers в репо
    — не задет).
  - **Побочная находка (НЕ почин, вне зоны этой правки)**: `nova_tests/plan83_10/
    handler_isolation_per_fiber.nv` падает ICE `[P67-LEGACY] Path call return type unknown for
    method=now` (`emit_c.rs:48941`) — эффект-хендлер `with Time = effect Time {...} { Time.now() }`,
    **нулевого отношения к Channel** (подтверждено: код-ревью показал, что диф (a)/(b) не трогает
    generic Path-call return-type dispatch; сам файл не использует Channel вообще). Pre-existing,
    независимый баг — не заведён отдельным маркером в рамках этой волны (не моя зона), но стоит
    завести при следующем заходе в эту область.

**NEW codegen-баги (обнаружены при Ф.2-enh + Ф.3, кандидаты на fix, вне .nv):**
- ~~**[M-codegen-nominal-type-name-collision]**~~ ✅ **CLOSED 2026-07-06 (D381).** Collision-aware module-qualified mangling приземлён (см. `[M-sync-crossmodule-samename-type-collision]` выше). Одноимённые cross-module типы (`ErrorKind` × io/http/compress) сосуществуют в одном CU: `Nova_<modpath>_<Name>` для коллидирующих, byte-identical для прочих. Разблокирует auto-decompress co-presence (http+compress `ErrorKind` в одном CU компилятся/линкуются — conformance PASS 1/0) + `ErrSource.Compress`.
- **[M-codegen-method-return-turbofish]** ✅ **CLOSED 2026-07-06.** generic-МЕТОД с type-param только в return-позиции игнорил turbofish `resp.m[T]()` → монеморфизировал в `nova_int` (silent `void*` miscompile). Root: concrete-record sentinel-mono-путь (emit ~28704 + оба infer sentinel-сайта) считал type-subst из receiver+args, дефолтя несвязанное в `nova_int`, НЕ смотря turbofish. Fix: (a) emit — seed unbound method-level type-params из `current_method_turbofish` (positional, None-slots) перед nova_int-fallback (mirror `resolve_method_level_subst`); (b) infer — передать `turbofish_args` (было `&[]`) в `resolve_mono_type_args`; (c) consume-checker (`types/mod.rs` `consume_walk_expr`) — unwrap turbofish в Call-арме, иначе `consume @json_as[T]()` не consumed receiver → ложный D133. Deliverable: `std.http.serdejson` получил метод-форму `Response consume @json_as[T Deserialize]() -> Result[T, HttpError]` (decode инлайнен — делегация в `json_decode_body[T]` триггерит отдельный открытый bound-forward gap `[M-176-io-forward-bounded-generic]`); free-fn остаётся substrate. Tests: `nova_tests/plan176_holes/m176_method_return_turbofish.nv` + 2 метод-формы в `nova_tests/http_typed/typed_json_test.nv`.
- **[M-codegen-serde-vtable-forwarddecl]** (P2) — serde `Serializer`/`Deserializer` protocol-vtable эмитит `Result[(),SerError]` без forward-decl → `unknown type` в большом multi-file-module CU. Обход: изоляция serde в отдельный узкий модуль.
- ~~**[M-codegen-cross-module-ctor-emission]**~~ ✅ **FIXED 2026-07-06.** Root (уточнён репро): explicit-receiver **payload-variant CALL** `Sum.Variant(x)` (`NetError.IoError(msg)`) парсится как 2-сегментный `Path` и в `emit_call` диспатчится через `method_overloads`-static-ветку — где payload-вариант зарегистрирован КАК pseudo-static-overload с `c_name = Nova_<Sum>_static_<Variant>` (никогда не определён; определён только `nova_make_<Sum>_<Variant>`). **НЕ** зависит от co-present одноимённого ТИПА (репро одинаков с/без `import std.io` — прошлый диагноз был неточен: не variant↔type clash, а universal explicit-receiver-payload-variant misroute). Unit-варианты (`NetError.ConnectionReset`, member-access, не call) не задеты. **Fix:** хелпер `try_emit_explicit_variant_ctor(recv_type, variant, args)` — когда receiver = сумма, владеющая payload-вариантом `variant` подходящей арности (не generic; collision-aware base через `ref_type_base`), эмитит `nova_make_<sum>_<variant>(args)`. Вставлен в ОБА static-emit-сайта (Path-арм до `method_overloads`-lookup + Member `method_receivers`-арм). Вариант всегда бьёт одноимённый static/тип-в-скоупе (контекст однозначен — вариант принадлежит сумме). Доказано: `nova_make_NetError_IoError` эмитится (0 undefined static-ref, baseline=1); net+http servernet CU линкуется. Delta-0 (http/compress/io/fs).
- **[M-codegen-multifile-module-impl-synth]** (P3) — `#impl(Deserialize)`-тип в multi-file-module CU: инстанцирование `f[T]` из другого файла того же модуля не видит synth `.deserialize` (single-file OK). Обход: изолировать в свою папку-модуль.

**Pre-existing compiler-баги (обнаружены при Ф.2, блокируют части плана — кандидаты на fix):**
- **[M-178-mock-handler-gc-trace]** (P2): handler-closure-env (`effect X {..}` из fn, captured heap-state) не GC-rooted → conservative Boehm собирает mid-run → segfault. Обход: inline-handler (frame-capture). Fix = root-registration closure-env в runtime/codegen.
- **[M-178-conformance-d357-d360-forwarddecl-bug]** (P2): forward-decl return-тип unit-возвращающего closure-call-fn (`fn f(b fn()){b()}`) мис-выводится в value-тип при наличии value-типов в CU → `conflicting types` CC-FAIL. Блокирует d357_*/d360_* single-CU conformance (покрыто nova_tests/http*). Fix = `return_type_c`/`infer_expr_c_type` для unit-closure-call.
- **[M-178-with-tail-bang-codegen]** (P3): `with{ ... X!! }` (tail unit-`!!`) → interrupt-ret-тип vs block-value-тип CC-FAIL. Обход: tail = `assert`/`()`.
- **[M-178-effect-op-result-monomorph]** (P3): прямой `Eff.op()->Result[A,B]` (A≠B) мис-типит Err-payload. Обход: fn-обёртка.
## [M-181-pattern-var-rebind] — Rebind pattern-bound var внутри matching-ветки (2026-07-04) — P3

`if Some(u) = e { ro u = … }` (аналогично while-let/match/for-loop var) не поддержан:
чекер уходит в stack-overflow (pre-existing — воспроизводится на baseline d97c0dbe,
независимо от Plan 181). Alpha-rename (Plan 181) СПЕЦИАЛЬНО не уникализирует такой rebind
(`Scope::pattern_origin`), чтобы форма давала legacy `redefinition` CC-error, а не
codegen-panic. Честный фикс — в чекере (172.1-зона): устранить overflow + аннотировать
канал resolved_types для rebind в pattern-scope. Home: 172.1 / отдельный заход.

## [M-consume-nested-scope-shadow-leak] — Nested-scope double-consume-shadow leak (2026-07-04) — P3

`consume tx = begin(); { consume tx = begin() } tx.commit()` (и то же в теле
if/for/match/while-let) — первый `tx` утекает МОЛЧА: consume-obligations ключуются по
имени, один `commit` гасит оба обязательства. Plan 181 R2 (`E_REBIND_LIVE_CONSUME`) ловит
ТОЛЬКО **same-scope** double-consume (alpha-rename по R7 НЕ уникализирует cross-scope →
`Module.rebind_shadows` для block-shadow пуст → `check_rebind_live_consume` early-return).
**Pre-existing** — воспроизводится ИДЕНТИЧНО на baseline d97c0dbe, независимо от Plan 181
(no-op-путь), НЕ регрессия 181; заголовок Plan 181 «catches B2 double-consume-shadow leak»
корректен для same-scope, но не покрывает nested. Честный фикс — в consume-чекере
(`types/mod.rs`): scope-aware obligation-tracking для block-shadow (D131/D133-территория),
НЕ alpha-rename (cross-scope затенение легально по R7, уникализировать его нельзя). Home:
D131/D133 consume-checker / отдельный заход. **Проверено 2026-07-07 (заход [M-into-raw-generic-stub-ret]):
НЕ тот же корень, что json-StringBuilder-краш** — тот оказался codegen pointer-stride (`*mut T`→`Nova_T**`),
здесь — checker-звучность consume-obligations; ОСТАЁТСЯ ОТКРЫТЫМ.

## [M-181-lsp-rename-symbol-table] — LSP rename over same-scope rebind (2026-07-04) — P3

LSP rename (D297 V1, word-boundary scan) переименует ОБА одноимённых same-scope биндинга.
Pre-existing долг (уже сломан для nested shadow); D347-rebind учащает. Честный фикс = V2
symbol table. Home: plan-104.6 followups.

## [M-181-w-shadow-unrelated-lint] — R5 warn W_SHADOW_UNRELATED (2026-07-04) — P3

Plan 181 Ф.4 R5: warn, когда rebind НЕ использует старое значение И старый биндинг жив/не
потреблён (`ro x = user; … ro x = socket`). Отложен как проектное решение: R2 (hard-error
`E_REBIND_LIVE_CONSUME`) закрывает soundness-критичный consume-случай; R5 — чистый style-warn,
который сам план флагует как спорный (Go-урок «shadow-линтер слишком шумный для дефолта»).
Реализация: детект в lints.rs (RHS не упоминает shadowed-имя из `Module.rebind_shadows` И
old не Consumed) + решение Ф.4.3 (маркер `EXPECT_WARNING <substr>` в test_runner ЛИБО
CLI-тест stderr). Подавление `#allow(shadow)`. Не гейтит корректность. Home: отдельный заход.
## Plan 180 (serde) followups — 2026-07-04

Record-path landed (Ф.1/Ф.2-record/Ф.4). Open:

- **[M-126-sum-equal-rich]/-clone-rich/-hash-rich** — sum rich auto-derive infra (OPEN on main; auto_derive.rs sum-arms = placeholders). GATES **Ф.2-sum + Ф.5 (enum-tagging)** = sub-plan 180.2 (D345). NOT on Plan 178 critical path (record-DTO suffices).
- **[M-180-serde-attributes]** — `#serde(rename/rename_all/skip/default/flatten/deny_unknown_fields/tag/content/untagged)` (Ф.3a parser+AST+validation → Ф.3b synth-consume). Record-DTO round-trips on canonical field names without it.
- **[M-180-bytes-base64] — ✅ CLOSED 2026-07-05 (record-path completeness audit).** `[]u8`/`Vec[u8]` field → base64 string wire (Q9, D-canon). Prior text was WRONG ("routes through generic seq" — it did NOT compile: `.serialize` on `nova_byte` ICE'd). Fix: the record synthesizer SPECIAL-CASES a byte-seq field (`is_byte_seq_ty`) to emit `s.serialize_bytes(@f)?` / `sub.deser_bytes()?` directly. Round-trip PASS (`nova_tests/serde/autoderive_ext.nv`, `raw []u8`). Scope: TOP-LEVEL byte-seq field; NESTED (`Option[[]u8]`) → typed error (see [M-180-container-narrow-scalar]).
- **[M-180-char-serde]** — `char` field serde. No faithful JSON scalar (codepoint-int vs 1-char-string ambiguity); today `char`/`i128`/`u128`/retired-`byte` fields are REJECTED with a typed `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL` (never an ICE — `serde_supported_scalar`). Followup: pick a canonical `char`↔wire mapping.
- **[M-180-container-narrow-scalar]** — narrow scalars (`i8..i64`/`u8..u32`/`uint`/`f32`) NESTED inside a container (`Option[i32]`, `Vec[i32]`, `HashMap[str,i32]`, tuple) → typed `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`. Direct-field narrow scalars are fully supported (synthesizer inlines `s.serialize_int(@f as int)?` + range-guard), but a container's generic body dispatches `v.serialize(s)`/`T.deserialize(sub)` on the ELEMENT and a narrow primitive does not dispatch as a method inside a generic mono. Container-safe scalars = `int`/`u64`/`f64`/`bool`/`str`. Fixing needs primitive-method mono inside generic container instantiation.
- **[M-180-namespace-static-generic-mono]** — turbofish on a namespace/type-static GENERIC method (`Serde.decode[T]`) does not monomorphize (emits erased symbol); realized public API is FREE functions (`json_encode`/`json_decode`). Fixing would allow the `Json.decode[T]`-style namespace API.
- **[M-180-streaming]** / **[M-180-arbitrary-precision-numbers]** / **[M-180-zero-copy-borrow]** / **[M-180-runtime-cycle-detection]** / **[M-180-nonstring-map-keys]** / **[M-180-backends-toml-yaml-binary]** / **[M-180-schema-gen]** — §11 "потом" (unchanged from plan).

Note — several codegen gaps discovered during Ф.2 were FIXED (not deferred): value-record-receiver+generic-method (`[M-180-valuerecord-receiver-generic-method]` CLOSED), static-method return-type inference for Path/Member/typevar receivers (`[M-180-static-method-path-ret-infer]` CLOSED), primitive-instance-generic-method mono (`[M-180-primitive-instance-generic-method-mono]` CLOSED), turbofish free-fn Result/Option return-resolve.

**Completeness audit close-out (2026-07-05).** An adversarial audit found the record-path "без упрощений" claim incomplete — six real defects, all now FIXED (not deferred): (1) `Option[value-record]` mono-struct ordering (`NovaOpt_NovaValue_Inner` embedded by value in a value-record hoisted ahead of the struct in `value_record_defs_buf` — родич keystone `[M-valuerecord-result-vtable-mono]`); (2) `HashMap[str, value-record]` pointer-field early forward-decl; (3) `Option[Option[int]]` nested-Option deserialize (recursive inline null-check + typed `None as Option[T]` construction) and `Option[Vec[str]]` serialize (`.serialize` receiver-invariant return-type fallback); (4) narrow scalars i8..i64/u8..u32/uint/f32 (synthesizer-inline ser/deser + range-guard; char/i128/u128 → typed error); (5) `[]u8`→base64 ([M-180-bytes-base64]); (6) encode-side lossy-integer guard (`SerLossyInteger`, symmetric with `is_exact_int`). Also fixed a latent `?`-lowering no-op that silently swallowed serialize errors in `json_encode`, and an early-prototype gap for structural `nova_opt_eq` on `Option[Vec[str]]`-carrying value-records. Gates: conformance 38/0; extended round-trip suite PASS (`autoderive_ext.nv`); neg typed-error tests PASS; zero-regression vs parent 64675407 (46-file sample, 0 deltas).

- `[M-180-emitc-infer-patches-to-channel]` (P2, §0-долг, из adversarial-аудита 180): 4 инференс-патча 180-го в emit_c.rs (infer_static_method_ret ~36882 + 8 wiring-сайтов; turbofish fn_ret-skip ~39081/42571; resolve_result_option_ret ~14033; primitive-instance-generic-method mono ~24510/27600) — интерим-re-derive (углубляют annotation-free инференс), НЕ целевая форма §0/§1 (checker → resolved_types-канал). Латентные векторы: (a) static `-> Self` лоуэрится через current_receiver_type ВЫЗЫВАЮЩЕГО — generic-static `-> Self` из чужого метода получит чужой C-тип (сейчас маскируется name-exclusion from/try_from/try_into/try_parse — §3-анти-паттерн); (b) turbofish-skip меняет fallback-цепочку для ВСЕХ turbofish generic free-fn; (c) resolve_result_option_ret лоуэрит сырой TypeRef (D315-анти-паттерн) + регистрирует NovaRes/NovaOpt как побочку инференса; (d) serde-inject вшит в 2 пайплайна вручную, в cmd_compile идёт ДО import-инлайна → cross-module serde-DTO может дивергировать test_runner vs nova-codegen compile; (e) bound-satisfaction impl_protocol_types собирается из module.items+peers — импортированный #impl(P)-тип не удовлетворит [T P]. Target: мигрировать все 4 на чтение checker-канала (serde-inject-before-check уже аннотирует тела — канал есть), unify serde-inject в один пайплайн-хелпер. Эмпирика на момент заведения: 0 регрессий (339-тест diff), риски латентны.

- `[M-153.1-append-as-slice-ccfail]` ✅ **CLOSED 2026-07-06.** `nova_tests/plan153_1` (folder-CU, `generic_overload.nv`) CC-FAIL `passing 'const nova_str' to parameter of incompatible type 'nova_int'`: НЕ append/as_slice — это `Box6[T] @tag(n int)` / `@tag(s str)` (same-arity param-type overload на generic-типе), обе схлопывались в ОДНУ `generic_type_methods["Box6"]` entry. Root (прямой разбор, не merge-бисекция): дедуп «та же декларация re-supplied builtin+import» сравнивал только name+param-COUNT+receiver, НЕ param-ТИПЫ → `tag(int)`/`tag(str)` считались дублями, второй отбрасывался → overload-selection видел `same_name.len()==1` → index 0 → оба call-site в первый mono `Box6____nova_int_method_tag`, `nova_str`→`nova_int` param. Fix: дедуп сравнивает и param-типы через новый span-free `type_ref_overload_key` (TypeRef не derive'ит PartialEq). Genuine param-overloads distinct; та же декларация из builtin+import — identical param-типы → всё ещё дедупится. Existing `generic_overload.nv` (repro) red→green.

- `[M-net-redesign-owner-directive]` (P1, директива владельца 2026-07-06): «Сеть мне не нравится, как реализовано — как всё сделаешь, будем переделывать сеть». Переделка std/net после завершения трека А (172.12/172.13). Перед стартом получить у владельца критерии (API / libuv-слой / модель эффектов). Связанное, что войдёт в переделку: P67-LEGACY ICE на net-путях (plan83_12 bind; исходный `[M-178-servernet-live-net-substrate-segfault]` segfault уже закрыт Планом 183, teardown-ordering — `[M-net-close-teardown-hang]` закрыт 2026-07-11), комбинатор таймаута (эффект-полиморфный deadline — сделать ДО переделки, пригодится). План 116 (TLS) — ПОСЛЕ новой сети, не на старой.

- `[M-d406-retract-leading-pipe]` (P2, 2026-07-06): по D406 маркер `enum` обязателен, но парсер ПОКА принимает старый leading-`|` без диагностики (потому 7 свежих std-файлов и проскочили — исправлены). Ретракция старого вида (ошибка парсера + подсказка) — ПОСЛЕ миграции корпуса тестов (план 182: nova_tests массово на старом виде). Агентам до ретракции: суммы писать ТОЛЬКО `type X enum ...`.

- `[M-time-default-handler-not-wallclock]` ✅ **CLOSED 2026-07-06.** Default-обработчик `Time.now_unix_ms()` (`nova_rt/fibers.h`, `_nova_time_default_now()`) вызывал `_nova_monotonic_ms()` (`uv_hrtime()`-based uptime) вместо настоящего wall-clock — `Timestamp.now()` в боевом режиме (без `with Time = handler`) отдавал ложный эпох. Fix: новая `_nova_wall_unix_ms()` через `uv_gettimeofday(uv_timeval64_t*)`; `_nova_time_default_now()` переключён на неё (автоматически чинит `now_unix_ms`/`now_ms`/`now_ns` default-путь — все три делегируют туда). Monotonic (`now_monotonic_ns`) не тронут. Тест-детектор в `std/time/units_test.nv` (`Timestamp.now() > 1_700_000_000_000` без handler'а). Spec: D316 amend (`spec/decisions/04-effects.md`). Гейт: cargo build clean; conformance 54/0 (не тронут); `std/time/units_test.nv` + `std/concurrency/supervised_deadline_test.nv` PASS; delta vs baseline a3a4da52 на `nova_tests/time`+`nova_tests/concurrency` — 0 непредвиденных регрессий (только path-prefix diff, идентичные pre-existing CC-FAIL/CODEGEN-FAIL — `_repro_p110`, `plan175_f1_timer_metrics_split`).

- `[M-into-raw-generic-stub-ret]` ✅ **CLOSED 2026-07-07.** Дефект, найденный волной D410 (блок 1), из-за которого откатили StringBuilder-переписывание json-сериализатора (см. коммит 93c3919a9): ≥3 соседних fn, каждая `consume buf = StringBuilder.new(); …; buf.into_str()`, одна вызывает другую при живом buf → детерминированный RUN-FAIL собранного теста (segfault). **Корень — НЕ consume-трекинг** (это отбрасывает гипотезу «одноимённые consume-биндинги»). `into_str()` лоуэрится в `str.from_bytes_unchecked_steal(@buf)` (`std/runtime/string/core.nv:241`), тело которого — `mut buf = bytes.into_raw()` (`Vec[T] @into_raw() -> *mut T`). `infer_expr_c_type` брал тип локали из checker-канала `fn_ret_by_span`, а канал ключуется по СПАНУ ДЕКЛАРАЦИИ callee → отдал НЕмонотипизированный `*mut T` = erased-заглушку `Nova_T**` (opaque struct, шаг указателя 8 байт) вместо конкретного `nova_byte*` (шаг 1). C-доказательство: `Nova_T** buf = Vec____nova_byte_method_into_raw(bytes);` → `(buf)[n] = 0` (in-place NUL-терм на has_room-пути) писал 8-байтовый NULL по смещению `n*8` — heap-overflow, портивший GC-кучу; крах проявлялся ниже по потоку (`nova_str_eq`→`memcmp` по NULL при `parsed == v`) когда несколько таких steal'ов накапливались (SEGV-DIAG backtrace: keystone = `Vec____Nova_JsonValue_p_method_equal`). Fix (`emit_c.rs`, `infer_expr_c_type` Channel 1 + Channel 2): для CALL пропускать канал-возврат, если это generic-заглушка (`debt_is_generic_stub_c`), и падать в receiver-subst-aware method-return инференс → `*mut T` резолвится в `nova_byte*`. Emitted-C: `nova_byte* buf = …`, `(buf)[n]=0` = корректная 1-байтовая запись. **Судьба соседа [M-consume-nested-scope-shadow-leak]: НЕ тот же корень** — тот про checker-звучность consume-obligations в nested-scope (same-name), мой — codegen pointer-stride; сосед ОСТАЁТСЯ ОТКРЫТЫМ. json-перф-долг снят: `format_string`/`format_array`/`format_object`/`pretty_array`/`pretty_object`/`indent` переписаны на StringBuilder+`into_str` (без `buf = buf + x` в циклах). Гейты: оба крейта clean; conformance 54/0 (baseline 54/0 — d102 CC-FAIL был ложным от моего первого enum-based теста, см. ниже; финальный тест enum не использует); `std/encoding/json_test` 24/24; `nova_tests/buffers/{roundtrip,write_misc}` + `plan108_1_mut_param_stringbuilder_ok` PASS; codegen-delta json_test.c vs base = ТОЛЬКО `Nova_T**`→`nova_byte*` (остальное — pre-existing HashMap-order флак match-арм авто-eq, подтверждён base-vs-base 30 строк; флака≠регрессия). Позитив: `spec_tests/conformance/d179_stringbuilder_cross_fn_consume.nv` (+4 юнита в positive-CU).

- `[M-crossfile-recursive-enum-autoderive-eq-int-lit]` (P2, найден 2026-07-07 при написании conformance-теста для [M-into-raw-generic-stub-ret]): рекурсивный `type X enum … | V([]X)` с авто-derive структурного `==`, добавленный ФАЙЛОМ в folder-CU `spec_tests.conformance`, ломает codegen СОСЕДНЕГО файла (`d102_named_args_default_params.c:34507`: `member reference type 'nova_int' is not a pointer`) — авто-eq lambda сравнивает `Nova_X* x` с мис-лоуэренным `((nova_int)3LL)` и делает `->tag` на int-литерале. Воспроизводится с enum-версией d179 И на base (1c16867ef) И с моим fix → **pre-existing, НЕ регрессия [M-into-raw]**. Обход в тесте: переписал d179 без кастомного enum (плоские `[]str`-функции — тот же cross-fn StringBuilder-паттерн). Home: отдельный заход (cross-file mono/lambda-нумерация в folder-CU).

- `[M-match-guard-trailing-paren-legacy-lambda]` (P2, найден 2026-07-07 при Plan json-byte-peek — std/encoding/json.nv lexer byte-санация @peek): match-arm guard, чьё условие заканчивается ГОЛЫМ parenthesized-выражением (не вызовом функции) непосредственно перед `=>`, ложно диагностируется как ретрактированный legacy-lambda `(x) => body` (Plan 19 D22-rev): `error: legacy lambda '(x) => body' removed in Plan 19 D22-rev — use closure-light '|x| body' or closure-full 'fn(x T) -> R body'`. Repro (минимальный, 2 варианта): `y if y == (1 + 2) => true` — FAIL на `=>`; `y if y == g(1) => true` (g — функция/const, без голых скобок перед `=>`) — PASS. Инференс корня (код парсера не проверялся): детектор legacy-lambda триггерится на «closing-paren непосредственно перед `=>`» без проверки, что paren-группа НЕ прикреплена к предшествующему идентификатору-вызову (`IDENT(args) => ...` не триггерит) — а сама позиция (guard-condition) грамматически лямбду не допускает структурно, т.е. false positive. Затронуло byte-based сравнения `Some(b) if b == ('{' as u8) => ...` (D54 char-литерал→u8 каст, иначе легальный) — 24 guard-arms в json.nv. Обход в json.nv (НЕ фикс парсера): именованные `const B_* u8 = 0x.. // 'x'` вместо inline-каста — guard заканчивается голым идентификатором, не `)`. Repro сохранён вне репо (scratchpad, не в коммите). Home: отдельный заход на парсер (legacy-lambda detector, вероятно рядом с Plan 19 D22-rev retraction кодом).

## Слито из docs/backlog-followups.md (рассинхрон двух задачников закрыт 2026-07-06; старый файл удалён, ссылки переведены)



## Слито из docs/backlog-followups.md (рассинхрон двух задачников закрыт 2026-07-06; старый файл удалён, ссылки переведены)

- **[M-single-letter-type-ban]** CLOSED Plan 167. Запретить `type X { ... }` где имя типа длиной 1 символ.
  Мотивация: однобуквенные имена конфликтуют с generic-параметрами (`fn[S Iter[T]]` vs `type S`),
  вызывая E_PREFIX_SHADOWS_NAMED_TYPE. Haskell решает регистром (type vars строчные), Nova
  решает запретом однобуквенных типов — generic-параметры остаются однобуквенными по конвенции.
  Реализация: новый error E_TYPE_NAME_TOO_SHORT в checker (name.len() == 1 для TypeDecl).
  Sweep: grep `^type [A-Z] ` по nova_tests/ и std/ — исправить (~10 в nova_tests/plan118_1_addr_chains/).
  Priority: M.

- **[M-prelude-name-shadow-hint]** Улучшить диагностику когда пользовательский тип называется так же как prelude-протокол.
  Сейчас: `type Iter { ... }` в модуле + использование в generic bound → `E_BOUND_NOT_PROTOCOL` (технически верно, но неясно почему).
  Хотим: hint «type name `Iter` shadows prelude protocol `Iter` — rename your type or use a qualified path».
  Реализация: в check_bound_ref, если bound-name резолвится в user TypeDecl (не Protocol) И в prelude есть Protocol с тем же именем — добавить hint к E_BOUND_NOT_PROTOCOL.
  Priority: M.

- **[M-vec-shadow-leak-e7310]** User-shadow обобщённого типа протекает во внутренние type-refs
  импортированного модуля. `type Vec { x int, y int }` (не-generic) в пользовательском модуле,
  затеняющий прелюд/импортированный `Vec[T]` (D29 «user wins»), приводит к тому, что СОБСТВЕННЫЙ
  код `std/collections/vec.nv` / `hashmap.nv` (`Vec[T]`, `Vec[Slot[K,V]]`) резолвится на
  пользовательский НЕ-generic `Vec` (0 type-параметров) → `[E7310] type Vec is not generic —
  takes no type arguments, but 1 was provided`. Затенение должно быть scope'нуто к модулю
  пользователя, не протекать в чужие модули. Комментарий fixture'а (plan138_2/t14) утверждает,
  что это когда-то чинилось → вероятно регресс (или дрейф от Vec-prelude-flip). Вскрыто
  консолидацией 169.1.2; обходной путь применён — shadow-fixtures plan138_2 (t14/t15/t16)
  переименованы в `UserRecNN` (shadow-покрытие снято, см. 169.2). Priority: M.

- **[M-D215-defaults-handler-lambda-type]** `infer_handler_interrupt_ty` не может вывести тип
  lambda-параметра `e` в паттерне `with Fail[E] = |e| interrupt Some(e) { ... None }`.
  Корень: `infer_expr_c_type(Lambda(...))` не знает тип `e` без binding annotation или
  type-propagation от `Fail[E]` окружающего контекста. Следствие: `Some(e)` → `NovaOpt_nova_int`
  вместо `NovaOpt_ParseComplexError` → match на `Option[ParseComplexError]` падает.
  Тест в `std/_experimental/math/complex.nv` закомментирован.
  Fix: propagate Fail-binding type через context при выводе типа handler-lambda параметров.
  Priority: M (нужен для любого non-trivial Fail-bound error handler).

- **[M-147-ro-binding-index-freeze]** `ro a []int` → `a[i] = x` должен давать ошибку по P7
  («голый `ro r` = freeze, весь owned-граф»), но сейчас **разрешается**.
  Корень: `check_target_readonly` ветка `ExprKind::Index` проверяет только `tr.is_readonly()`,
  но не `ro_binding_names`. Для `ExprKind::Member` `is_through_ro_binding` есть — для Index нет.
  ВАЖНО: `a[i]=x` для `[]T` codegen-inlined (`Stmt::Assign + ExprKind::Index`), НЕ диспатчится
  через `mut @index` метод (vec/access.nv:53-54) — поэтому `mut_methods` реестр не помогает.
  Баг актуален сейчас для `[]T` + после Plan 121 для `[N]T`.
  Fix: добавить `is_through_ro_binding(obj)` в Index-ветку `check_target_readonly` + oracle-тест.
  Priority: M.

- **[M-147-ro-ro-redundant-binding]** Следующие формы должны давать `E_REDUNDANT_TYPE_MODIFIER`
  (D246 «Канон синтаксиса»), но сейчас принимаются без ошибки:
  - `ro a ro T` — явный `ro` на binding + явный `ro T` на типе
  - `func(a ro T)` — параметр ro по умолчанию (D176) + явный `ro T` на типе
  - `mut a mut T` — `mut` binding + явный `mut T` (тип без модификатора уже mutable)
  - `func(mut a mut T)` — то же для параметра
  Fix: в checker при let/param — если (binding ro явно или по умолчанию) И тип явно `ro T` →
  `E_REDUNDANT_TYPE_MODIFIER`; если binding mut И тип явно `mut T` → то же.
  Priority: M.

- **[M-147-param-index-freeze]** `func(a []int)` → параметр ro-binding по умолчанию (D176);
  `a[i] = x` внутри fn должен давать ошибку — codegen-inlined путь, не через `mut @index`.
  Связан с [M-147-ro-binding-index-freeze] — один и тот же фикс в Index-ветке `check_target_readonly`.
  Priority: M.

- **[M-138-vec-pointer-element-mono]** `Vec[*T]`/`Vec[*mut T]`: codegen монорфизация для pointer-element-type сломана — `Vec.new()` вызывает generic-заглушку `Nova_Vec_static_new()` → NULL вместо специализированного конструктора → SEGFAULT при push/index. Структура `Nova_Vec____int64_t_p` и методы push/index генерируются правильно; ломается только static constructor. `Option[*mut T]: Some(p)→*p=v` работает (другой путь). Воспроизводится: `mut v Vec[*mut i64] = Vec.new(); v.push(&a); unsafe{*v[0]=100}`. Priority: P2.

- **[M-168-resize-with-free-fn-shadow]** `plan153_1/resize_with_free_fn_shadow` — pre-existing CODEGEN-FAIL: `undefined identifier f` when a module-level free fn `f` clashes with closure param `f` inside Vec.resize_with/fill_with. Not caused by Plan 168. Requires fix in name resolution (closure param scope should shadow outer free fn). Priority: M.

- **[M-168-other-generic-fwd-decl]** Other generic types (HashMap[K,V], Set[T], etc.) may have similar body-only instantiation gaps if they're used in fn bodies but not in signatures/fields. The Plan 168 tuple-elem fwd-decl fix covers them too (via MONO_TUPLE_TYPEDEFS), but the pre-pass body-scan only scans Vec TurboFish. If HashMap[str, u32] appears body-only it may also fail. Monitor for CC-FAIL patterns and extend scan if needed. Priority: L.

- **[M-91.8b-precompiled-c-rebuild]** ✅ CLOSED (Plan 91.15, 2026-06-17) — plan91_8b 6/6 PASS.
- **[M-91.15-hashmap-precompiled-eq]** `std/collections/hashmap.c` (precompiled) still uses `k.eq(key)` struct-member syntax instead of `Nova_str_method_equal`. CC-FAIL on map_literals tests with str keys. Fix: regenerate hashmap.c via `nova build-std` after Plan 91.8b @eq→@equal rename. Priority: M.

- **[M-91.10-remove-needs-caps-field]** ✅ CLOSED (Plan 91.15 Ф.5, 2026-06-17) — FnDecl.needs_caps removed from AST.
- **[M-91.14-option-result-debug]** ✅ CLOSED (Plan 91.15 Ф.2, 2026-06-17) — Option/Result @debug work via DeclaredBody interp dispatch.
- **[M-91.14-derive-debug]** ✅ CLOSED (Plan 91.15 Ф.3, 2026-06-17) — `#impl(Debug)` auto-derive works for record types. known-limit: checker does not validate field Debug bounds at synthesis time.

- **[M-126-sum-compare-rich]** ✅ CLOSED (Plan 180 Ф.1, 2026-07-06) — sum `@compare` = variant-index order, then payload lexicographic. `synth_compare_sum_body`.
- **[M-126-sum-fmt-rich]** ✅ CLOSED (Plan 180 Ф.1, 2026-07-06) — sum `@display`/`@debug` = variant-aware output (`V` / `V(x, y)` / `V { f: x }`). `synth_fmt_sum_body`. Verify: `nova_tests/plan180_f1/sum_rich_autoderive_ok.nv`; behavior-change updated `plan91_14/pos_debug_sum_derive.nv` (was pinning V1 typename placeholder).
- Ergonomics known-limit: method on a bare unit-variant (`Nought.hash()`) mis-infers the variant as its own type — annotate via a `Self`-typed local. Pre-existing bidirectional-inference boundary, not a synth defect.

- **[M-180.2-sum-auto-derive]** ✅ CLOSED for externally-tagged (Plan 180 Ф.2-sum, 2026-07-06) — `#impl(Serialize + Deserialize)` on a sum synthesizes externally-tagged bodies: unit → `"V"`; single → `{"V": x}`; tuple → `{"V": [a, b]}`; record → `{"V": {fields}}`. `auto_derive.rs::synth_serialize_sum_body`/`synth_deserialize_sum_body`. Runtime: `Deserializer.@is_str()`, `DeErrorKind::UnknownVariant`/`NoVariantMatched`. Verify: `nova_tests/serde/sum_autoderive.nv` (8 blocks: 6 round-trip + 2 neg). Codegen fix: `.deserialize?` Result-type pin at the Try-lowering site (mono-collection order perturbation when a sum co-derives Deserialize).
- **[M-180-serde-tagging-modes]** ✅ CLOSED for internal + adjacent (Plan 180 Ф.6, D382, 2026-07-06) — `#serde(tag="k")` internally-tagged (`{"k":"V",…fields}`; tuple variant → E_SERDE_INTERNAL_TAG_NON_STRUCT) and `#serde(tag="t",content="c")` adjacently-tagged (`{"t":"V","c":payload}`) synthesize round-tripping ser/deser over the existing `Serializer`/`Deserializer` primitives; mode from `serde_tagging_mode(td)`. Untagged → `[M-180-untagged-codegen-mono]`. Verify: `std/encoding/serde/tagging_test.nv` (peer) + `std/encoding/serde_neg/*` (`nova test std/encoding/serde_neg --compile-error`). Needed a match/if `Result[OK,ERR]`-arm reconciliation codegen fix (`Ok(x)`/`Err(e)` each infer a half-stub; combine via `novares_ok_err`-split).
- **[M-180-untagged-codegen-mono]** OPEN (NEW, Plan 180 Ф.6) — `#serde(untagged)` derive is synthesized correctly (try-each-variant, C is valid) but compiling an untagged-derive body perturbs `std/encoding/json` codegen in the same compilation unit (mono-collection ordering → `Json.parse` mis-tags a number as a bool). Gated at compile time (`E_SERDE_UNTAGGED_GATED`) until the codegen mono-ordering is hardened. A compiler bug, NOT a serde-logic defect — internal/adjacent (same synth machinery) land unaffected. Repro: two sums in one CU, one `#serde(untagged)` deriving; `Json.parse("{\"c\":9}")` returns `Bool` for `9`.
- **[M-180.1-synth-fndecl-span-file-id]** ✅ CLOSED (Plan 180.1 Ф.1, 2026-07-22, found + fixed same wave, `p180-serde-field-attrs`) — **general auto-derive-machine gap, not serde-specific**, surfaced by `#serde(default = "fn_name")` (the FIRST synthesized body to reference an arbitrary lowercase user free-function by bare identifier — every prior synthesized reference is either a builtin/Capitalized name, exempt from the check, or a `.method()` call, resolved via the method table instead). Root cause: the identifier-resolution checker (`types/mod.rs::is_known`, Plan 42.15 Rule C) picks `file_id` **once per top-level `Item::Fn`, from that FnDecl's OWN `span.file_id`** — a sound assumption for ordinary parsed code (a function's body always lives in the same file as its declaration) but broken for a compiler-SYNTHESIZED `FnDecl`: `make_serde_method`/`auto_derive.rs` built its own span via `span_dummy()` (`MAIN_FILE_ID` = 0), so a type carrying `#impl(Deserialize)` synthesized for a module IMPORTED from elsewhere (the common case — a DTO declared once, decoded from a different file) had its bare free-function references checked against the ENTRY file's own declaration scope instead of the type's OWN declaring module's — spuriously "undefined identifier". Repro was 100% general (not test-file-specific): a minimal two-file package (plain production `mymod/types.nv` + `consumer.nv` importing+decoding it) reproduced identically; conformance mega-CU caught it live via `spec_tests/conformance/standalone/m196_serde_option_match_arm.nv` (`import std.encoding.serde` pulls the whole peer-folder in, including the new `field_attrs_test.nv` — CODEGEN-FAIL `undefined identifier default_role`). Fix: `make_serde_method` now takes the type-decl's own `file_id` and tags the synthesized `FnDecl`'s span (+ inner `Ident`/`call` nodes referencing the free function, via new `ident_at` helper) with it, instead of `span_dummy()`. Verify: `nova test spec_tests/conformance/standalone/m196_serde_option_match_arm` RED→GREEN; full conformance mega-CU FAIL:0 (see 180.1 gate tally). Only the SERDE synthesizer was touched (`make_serde_method`) — the other 6 auto-derive protocols (`Equal`/`Hash`/`Clone`/`Compare`/`Display`/`Debug`, `make_synth_method`) never reference an arbitrary bare free-function today so the same latent gap doesn't (yet) manifest there; worth the same fix if/when a future auto-derive feature needs it (noted, not applied preventively — out of this wave's scope).
- **[M-180-serde-field-attributes]** ✅ CLOSED for record types (Plan 180.1 Ф.1/Ф.10, D435, 2026-07-22) — `rename`/`rename_all`/`skip`/`skip_serializing_if`/`default`/`alias`/`deny_unknown_fields`/`allow_unknown` synth-consumption landed in `auto_derive.rs` (`resolve_fields`/`validate_wire_contract`/`build_field_with_fallback`/`build_unknown_field_check`); new `Deserializer.has_field`. Ф.10 wire-contract validations (`E_SERDE_WIRE_NAME_COLLISION`/`_SKIP_RENAME_CONFLICT`/`_ATTRIBUTE_MISPLACED`/`_ATTRIBUTE_ON_SUM_UNSUPPORTED`/`_UNKNOWN_FIELD_POLICY_CONFLICT`/`_SKIP_FIELD_NO_DEFAULT`). Scope: record/named-tuple fields only — `SumVariantKind::Record` payload fields still don't consume field-attrs (sum rich synth is `[M-126-sum-*-rich]`'s separate gate). Remaining sub-scope: `[M-180-serde-flatten]` below. Verify: `std/encoding/serde/field_attrs_test.nv` (peer, wire-string assertions); `std/encoding/serde_neg/*` 16/0 (`--compile-error`).
- **[M-180-serde-flatten]** OPEN (NEW, Plan 180.1 Ф.1.8, split out of `[M-180-serde-field-attributes]` 2026-07-22) — `#serde(flatten)` grammar + static validation land (D435), but SYNTHESIS is gated (`E_SERDE_FLATTEN_DENY_CONFLICT` when strict; `E_SERDE_FLATTEN_UNSUPPORTED` even under `allow_unknown`). Needs a companion "fields-only" synth variant (writes/reads the flattened child's fields directly into the PARENT's `s`/`d` cursor, without its own `begin_struct`/`end_struct`/`enter_field` framing) that the auto-derive machine does not yet emit — a structural extension (one method, two synth shapes), not a body tweak. Honestly scoped out rather than forced; ordinary (non-flattened) nested sub-object fields work today.

- **[M-147-ro-binding-index-freeze]** ✅ CLOSED (Plan 147 Ф.7, 2026-06-17) — `ro a = [...]; a[0] = x` now gives `E_READONLY_CONTENT`. `is_through_ro_binding` check added to `check_target_readonly` Index arm in `compiler-codegen/src/types/mod.rs`; entry-code guard avoids false positives in prelude/std imports.
- **[M-147-param-index-freeze]** ✅ CLOSED (Plan 147 Ф.7, 2026-06-17) — non-`mut` params are now registered in `ro_binding_names` at fn entry (snapshot/restore), so `v[i] = x` on a plain `v []int` param gives `E_READONLY_CONTENT`.
- **[M-147-ro-ro-redundant-binding]** ✅ CLOSED (Plan 147 Ф.7, 2026-06-17) — `ro a ro []int = [...]` gives `E_REDUNDANT_TYPE_MODIFIER`; handled at parser level (`parser/mod.rs` lines 5198–5205, already present); oracle test `f7_neg3` confirms.
- **[M-147-readonly-content-lsp-quickfix]** nova-lsp `E_READONLY_CONTENT` quick-fix (Plan 147 Ф.7, 2026-06-17) — базовый `fix_readonly_content` добавлен в `nova-lsp/src/code_actions.rs`: ищет `ro <name>` binding вверх по файлу и предлагает `ro → mut`, или добавляет `mut ` перед параметром. Priority: P2 (улучшить heuristic при необходимости).

- **[M-118.7-safe-addr-outside-fn-scope]** Plan 118.6/118.7 known limitation: `&ident` без `unsafe {}` как trailing expr в fn body даёт `undefined identifier` (checker ищет ident в другом контексте). Workaround: `unsafe { &ident }` — поведение идентично после 118.7. Priority: P3 (правильная fix requires full type-inference in escape sink).

- *(write-cap указателей → перенесено в [Plan 177](plans/177-pointer-ops-methods.md) Ф.1/§4; C-FFI ABI типы → [Plan 178](plans/178-ffi-abi-types.md). Были `[M-138.5-unsafe-ptr-write-cap]` / `[M-D282-ffi-abi-type-list]`.)*

- **[M-91.18-to-words-array]** `str @to_words() -> []str` — eager materialization of word segments (mirrors `to_chars`). Priority: P2.
- **[M-91.18-eq-u8-slice]** `Equal` for `ro []u8` — would simplify `string_builder.nv @starts_with/@ends_with` (`.compare(b)==0` → `==b`). Priority: P2.
- **[M-91.18-from-bytes-lossy-slice]** `str.from_bytes_lossy` valid-sequence push optimization: `out.append(bytes[i..i+seq])` instead of per-byte push. Priority: P2.
- **[M-91.18-validate-utf8-dedup]** Shared `utf8_seq_len()` helper to de-duplicate utf8 sequence-length calculation between `from_bytes_lossy` and `chars.nv` decode. Priority: P3.
- **[M-91.18-stringbuilder-len-naming]** Consider `@len` → `@byte_len`, `@capacity` → `@cap` on StringBuilder (aligns with str convention; WriteBuffer family naming context). Priority: P3.
- **[M-91.18-unicode-cat-enum]** `GCB_*` / `WB_*` / `GC_*` / `SB_*` constants as real enums (requires codegen enum-from-int support). Priority: P3.
- **[M-91.18-import-gated-str-methods]** `str @to_upper()` / `str @to_lower()` extension methods currently resolve without `import std.unicode` (str ext-methods bypass import gating). Fix would require per-module method visibility tracking in the resolver. Priority: P2.
- ~~**[M-152.5-collation-conformance-u32-overflow]**~~ ✅ **FIXED 2026-06-19.** `nova_tests/plan152_5/collation_conformance.nv` RUN-FAIL `array: index 12884901890 out of bounds for length 4` (= 3·2³²+2). Root cause: in `collate.nv` `s21_match`, the consumed-index list (`Vec[int]`) was pushed through `cp_seq_push(src Vec[u32], x u32)` — the `(hi<<32)|lo` garbage came from reinterpreting 64-bit ints as 32-bit u32 words. Triggered only on the DUCET **S2.1 discontiguous** contraction path (Tibetan U+0FB2+U+0F71+U+0F80). Fix: added `idx_seq_push(src Vec[int], x int)` and routed both `cur_consumed` pushes through it. Regression-guard added to `collation.nv`.
- ~~**[M-vec-elem-type-mismatch-silent]**~~ ✅ **FIXED 2026-06-19** (generalized to **[M-generic-arg-type-mismatch-silent]**, commit `a9726e91`). The checker accepted passing a whole generic value with a different concrete-primitive type-argument (`Vec[int]`→`Vec[u32]`, user `Stack[int]`→`Stack[u32]`, `Option[f32]`→`Option[f64]`, …) — a pointer reinterpretation that surfaced only as a runtime OOB or a late C-stage CC-FAIL. Root cause: `cat_of`/`TyCat` folds all int widths into one `TyCat::Int` AND drops a named type's generic arguments. Fix (general, NOT Vec-specific): `f1_check_call` compares each type-argument of matching generic types at raw-TypeRef granularity and emits `[E_ARG_ELEM_TYPE_MISMATCH]`. (Scalar `int`→`u32` coercion outside a generic is NOT touched by this check — but per spec it should require explicit `as`; the current lenient behavior is a SEPARATE gap, see `[M-scalar-nonliteral-narrowing-not-enforced]`.) Supporting: `cat_of` lowers named `Vec[T]`→`Array` (D239 `[]T ≡ Vec[T]`); `infer_expr_type` resolves `Type[T].{new,with_capacity,from,default,filled}(…)` to carry element types into scope. Tests: `nova_tests/vec_elem_type/` + `plan70_4/neg/`.
- ~~**[M-scalar-nonliteral-narrowing-not-enforced]**~~ 🟡 **MOSTLY DONE 2026-06-19** (commit `f96016e6`). Per spec D54+D227 a non-literal wider int narrowing into a narrower / value-range-unsafe int position now requires explicit `as` → `[E_IMPLICIT_NARROWING]`. Enforced at: **bindings** (`ro a u8 = int_var`), **free-fn / static-method arguments** (`take_u8(int_var)`), and **reassignment** (`a = int_var`). Rule: value-range-preserving widening stays implicit (signed→wider-signed, unsigned→wider-unsigned, unsigned→strictly-wider-signed, `int`≡`i64`, `uint`≡`u64`); narrowing + signed→unsigned + value-unsafe cross (u32→i32, u64→int) need `as`. Literals keep their D227 range-check; `as`-casts auto-exempt. **Blast radius was ZERO** (no std migration needed) — see the remaining gap below. Tests: `nova_tests/narrowing/`. Spec amend pending (D54/conversions.md — gated on the other session's in-flight spec edits to `03-syntax.md`).
- ~~**[M-instance-method-arg-scalar-narrowing]**~~ ✅ **CLOSED — Plan 172.2 (2026-06-26, commit `b2de1218c`).** Precise scope (corrected 2026-06-19 after empirical mapping): argument types of method calls ARE validated, just by other layers — overloaded fns/methods resolve by static arg types in the **codegen overload resolver** (emit_c.rs:23026; a no-match → CODEGEN-FAIL `no matching overload for g(nova_bool)`), and a category mismatch (struct↔scalar, e.g. `Vec[int].push(str)`) is caught by the **C compiler** (CC-FAIL `passing nova_str to incompatible type nova_int`). The Nova type-checker itself does not type-check method args, but the ONLY thing that slips through ALL layers is **scalar→scalar implicit narrowing** through a single-overload method arg (`vec_u32.push(int_var)`: arity matches the one `push(u32)`, and int→u32 is a C-legal truncation). Fix landed checker-side (Q1 variant C): `check_instance_overload` (types/mod.rs) substitutes the chosen overload's receiver type-args into each param and emits `[E_IMPLICIT_NARROWING]` on `Compat::Narrowing`, covering builtin `Vec.push`. The ~375 std `push(int)` sites were closed not via mechanical `as`-wraps but by flowing `codepoint = u32` end-to-end in std/unicode (D327). Fixtures: detect172 `u172_2_method_arg_narrowing_pos` + `neg/n_method_arg_narrowing`. Priority: P1 (soundness) — done.
- ~~**[M-generic-arg-mismatch-records-followup]**~~ ✅ **DONE 2026-06-19** (commit `4e5533ff`). The generic-argument mismatch check now flags concrete **record/sum/newtype** type-args too (`Box[Dog]`→`Box[Cat]`) and **nested** generics (`Vec[Vec[int]]`→`Vec[Vec[u32]]`) via a recursive `generic_arg_mismatch()`. Alias-safe (resolved via `cat_of`, so `Box[Meters alias int]`→`Box[int]` does not false-flag); permissive on generic type-params / protocols / unknowns. Zero false positives across the corpus.
- **[M-172.1-U1-cli-stdpath]** Plan 172.1 U.1.1: std-path is configurable via env `NOVA_STD_PATH` + `nova.toml [workspace]/[package].std` (resolver `manifest::resolve_std_path`, default `repo/std` byte-identical). The CLI `--std-path` flag (a third config surface above env) is not yet wired — env+manifest already satisfy the §2 «WHERE is config» requirement. Priority: P3 (UX convenience). Add a `--std-path` arg threaded (via a process-global set at startup) into `resolve_std_path`.
- **[M-172-nova-int-fallback-audit]** Plan 172 / conventions §1 «никаких авто-выводимых неверных типов». `emit_c.rs` имеет **~78** сайтов молчаливого fallback-типа (`_ => "nova_int"`, `unwrap_or(... nova_int)`) в путях вывода C-типа: при неизвестном типе codegen подставляет `nova_int` вместо резолва → **soundness-дыра** (маскирует ошибку типа: `if` на «int» вместо `bool`, мис-диспатч; всплыла на `self.try_start_won()` → `nova_int` при инлайне sync, см. [M-172.1-U1-lib-import-needs-U4]). Это симптом фрагментированного inference; **корректный фикс — U.4** (типизированный IR: чекер резолвит тип, codegen читает; genuinely-unresolvable → `[E_*]`-диагностика). НЕ патчить точечно в codegen (§0/§1). Audit: `grep -nE '_ => "nova_int"|unwrap_or.*nova_int' emit_c.rs`; каждый сайт при переносе на единый inference либо получает реальный тип, либо становится диагностикой. Priority: P1 (soundness). Gate: U.2→U.3→U.4.
- **[M-172.1-U2.3-synth-overlay]** Plan 172.1 U.2.3 (variant A; commits `930f3eda`/`12e492f6`/`0b225980`). Три контекста чекера (BoundCtx/CapabilityCtx/TypeCheckCtx) больше НЕ строят собственные `fn_decls`/`method_table` — читают ОДИН base-реестр `SigRegistry::build_base` (§0; три дублирующихся build-цикла устранены). Осознанный компромисс (F2): общий реестр = **base-only**; синтезированные auto-derive методы (Plan 126) остаются TypeCheckCtx-PRIVATE overlay `synth_methods` (НЕ в общем реестре — Bound/Cap не должны их видеть: их резолв base-only, byte-identical к до-рефактора). Поле `method_table` убрано из всех трёх; TypeCheckCtx сохраняет `synth_methods` (genuinely-unique забота auto-derive, НЕ дубль build-цикла). Полная унификация (вариант B: synth внутрь общего реестра + корпус-пруф, что Bound/Cap не затронуты) — возможный follow-up; A выбран как min-risk byte-identical. Priority: P3 (чистота §0; функционально завершено + byte-identical-верифицировано на 43 фикстурах зон риска вкл. plan126).
- **[M-172.1-U2.4-mangling-fragmented]** Plan 172.1 U.2.4 (разведка 2026-06-20). Исходная форма U.2.4 («standalone `SigRegistry` → populate `method_overloads` из неё») byte-identical НЕВЫПОЛНИМА: `method_overloads` строится из 5+ источников (ExternalRegistry builtins :2374 / free-fn D84 :3189 / receiver methods :3311 / embed-proxy D39 :3568 / mono :5650,:9560) с РАЗНЫМИ mangling-схемами — codegen использует `receiver_type_c_ident` + суффикс по ВСЕМ param-C-types (sanitized) + `__mut`/`__ro` tiebreak (Plan 135) + `erased_type_ref_c` (generic-recv) + `free_fn_c_name` (modular/file-priv/literal); SigRegistry (`mangle_method_c_name`+`last_param_suffix`) — упрощённая (last-param-Nova suffix, raw type, без mut/erasure/modular), совпадает лишь для single-overload concrete-recv (кейс parity-теста U.2.2). Плюс `ExternalRegistry::type_ref_to_c` (standalone) ≠ `CEmitter::type_ref_to_c` (state-aware). Корень: codegen mangling/type-map зависят от `CEmitter`-состояния (generic_types/mono/receiver-context/fn_module_map), которого нет у независимого реестра. Развилка: (1) строить SigRegistry ВНУТРИ CEmitter + единый mangler (целевая §0) / (2) вынести ОДИН shared mangler, источник не менять / (3) отложить U.2.4-impl за U.4/U.5 (typed IR) + U.6 (collapse `type_ref_to_c`×3); сейчас закрыть U.2.5 (fold MethodSig + del `resolve_overload`). Priority: P1 (§0 ядро). Gate: решение владельца + (для целевой) U.4/U.5/U.6.
- **[M-172-codegen-typedef-order-nondeterminism]** Pre-existing (обнаружено при U.2.3 byte-identical гейте, 2026-06-20). Codegen эмитит forward-typedef-блок (`typedef struct Nova_X Nova_X;`) в порядке HashMap-итерации → **порядок строк варьируется между запусками ОДНОГО бинаря** (подтверждено: 2 прогона одного `nova.exe` на одном входе дают разный порядок `Nova_U`/`Nova_F`/`Nova_K`). Семантически безвредно (forward-typedef порядок-независимы), но: (1) нарушает §2-детерминизм сборки; (2) ломает наивный byte-diff `.c` как verification-гейт (приходится сравнивать line-multiset, `diff <(sort a) <(sort b)`); (3) снижает эффективность `.c`-кэша (байт-идентичный вход → разный `.c`). Фикс: эмитить forward-typedefs в детерминированном порядке (BTreeMap / сортировка по имени / declaration-order items). Priority: P2 (детерминизм сборки + byte-identical-верифицируемость будущих рефакторов).

- **[M-169.2-vec-fn-empty-literal-nova-int]** `mut arr []fn() -> int = []` — пустой
  array-литерал для `[]fn` выводит element-type как **`nova_int`** (fallback), а не
  fn/void_p: codegen создаёт `Nova_Vec____nova_int_static_new()`, но `arr` типизирован
  `NovaArray_void_p*` и в него пушатся closure-указатели → type-confused контейнер.
  Малый N работает (совпадение layout), на масштабе (≥~512, realloc) расходится →
  элемент читается как null → `NOVA_CLOS_CALL_vi(null)` → детерминированный SEGV (READ@0,
  frame[1]=`nova_fn_main_impl`). **НЕ GC** (`GC_DONT_GC=1` не чинит). Это конкретный
  инстанс класса **[M-172-nova-int-fallback-audit]** (silent nova_int fallback на unknown
  element-type) → **гейтован на Plan 172 U.4** (removal of fallback). Репро: plan55
  `f1_closure_array_gc_stress` (RUN-FAIL 3/3); диагностика по docs/debugging-races.md §2.1.1.
  Priority: M (гейт 172 U.4).

- ✅ **FIXED [M-172-with-fail-swallows-panic]** (Plan 173 Ф.1, `25e07590`, 2026-07-04; home Plan 173 Ф.1) —
  общий helper `nova_scope_exit` введён, симметричная ветка для `NOVA_THROW_PANIC` добавлена ПЕРЕД
  USER-path (re-throw вместо swallow); `supervised{}`-граница для panic-restart не тронута (отдельная).
  Ниже — исходное описание находки (оставлено как обоснование фикса). `with Fail[E]`-handler **ловит `panic`** как
  recoverable-ошибку → **нарушение D13** (panic перехватывается ТОЛЬКО runtime'ом на
  границе fiber'а; «программист НЕ ловит panic в обычном коде», нет `try_panic`/`catch` —
  spec/decisions/08-runtime.md §«Три уровня катастрофы»). **Эмпирически подтверждено
  2026-06-20** (C-codegen): `panic("BOOM")` внутри
  `with Fail[E1] = effect Fail { fail(_e) { interrupt () } } { risky_panic() }` → with-блок
  отдаёт значение, выполнение продолжается. Сырой stdout = `PROBE\nREACHED_AFTER_HANDLER`,
  процесс жив (exit 0), `panic: BOOM` НЕ всплыл. Ожидалось: паника проходит сквозь
  Fail-handler до границы fiber'а — в синхронной CLI = смерть процесса с `panic: BOOM`.
  **Root cause:** re-dispatch ветка Fail-handler'а (`emit_c.rs:6648-6675`) ре-throw'ит
  ТОЛЬКО `NOVA_THROW_CANCEL`; `NOVA_THROW_PANIC` проваливается в «USER path: handler already
  ran» → паника проглатывается (а CANCEL — единственный структурный throw, который
  корректно пробрасывается). **Фикс:** добавить симметричную ветку `if (ff.error_kind ==
  NOVA_THROW_PANIC) { nova_fail_pop(); nova_interrupt_pop(); restore handlers; nv_panic(ff.error_msg); }`
  ПЕРЕД USER-path (NB: `supervised{}` ДОЛЖЕН продолжать ловить panic для restart — это
  ОТДЕЛЬНАЯ граница, не трогать). Priority: **P1** (soundness — panic recoverable вопреки D13).
  Репро (scratch, удалён — пересоздать при фиксе как `EXPECT_RUNTIME_PANIC BOOM`):
  ```nova
  module nova_tests.<stem>
  type E1 { msg str }
  fn risky_panic() Fail[E1] -> () { panic("BOOM") }
  fn main() -> () {
      println("PROBE")
      with Fail[E1] = effect Fail { fail(_e) { interrupt () } } { risky_panic() }
      println("REACHED_AFTER_HANDLER")   // НЕ должно печататься после фикса
  }
  ```

- **[M-181-ifexpr-value-materialize-codegen]** ✅ **RESOLVED 2026-06-26** (Plan 172.1, commit
  `836befcb`, ветка `plan-172-unified-type-engine`). `else if`-цепочка, где хвост ветки — fluent
  `-> @`-метод (`out.push(...)`), а финальная ветка диверджит → каст `(NovaArray*)(nova_unit)` =
  CC-FAIL (base64 `decode_with`, `base64.c:6426`). **Корень УТОЧНЁН эмпирически (НЕ «receiver-vs-
  return» из первичного owner-insight — `push` fluent `-> @`, ЛЕГИТИМНО возвращает `Vec*`):**
  рассинхрон emit/infer. `emit_if_expr` имеет fallback unit-доминирования
  `[M-codegen-fluent-tail-if-unify]` (свернуть if в `nova_unit`, когда одна ветка fluent-value,
  сиблинг unit, statement-позиция); `infer_If` (`emit_c.rs:38399`) его НЕ имел, хотя арм явно
  требует «must match emit_if_expr's choice» (R3). Вложенный if эмитит unit, но `infer_If`
  возвращал `Vec*` → внешний if типизирует result-temp как `Vec*`, присваивает unit → CC-FAIL.
  **Фикс:** `infer_If` вычисляет `(else_diverges, else_ty)` симметрично и применяет тот же
  fallback — точное зеркало `emit_if_expr` (§0 один резолвер, восстановление R3). НЕ U.4-канальный
  флип (первичная привязка к U.4.4 была основана на неточном диагнозе). Гейт: base64+cgfix(chain)
  CC-FAIL→PASS, §7.5 0 регрессий, §0 GOLD multiset-.c 5 диров IDENTICAL, regression-фикстура
  `cgfix_fluent_tail_if` (chain). Priority: P2 → DONE.
- **[M-181-result-over-named-tuple-codegen]** ✅ **RESOLVED 2026-06-26** (`b022919a` фикс + `a2d01a67`
  миграция, ветка `plan-172-unified-type-engine`). `Result[T,E]` (и `Vec`) над **named-tuple**-типом
  (`type Complex(re f64, im f64)`) → wrapper `NovaRes_NovaTuple_Complex_…` встраивал `NovaTuple_Complex`
  в Ok-payload **ПО ЗНАЧЕНИЮ**, но эмитился в РАННЕЙ `__NOVARES_TYPEDEFS__` ДО typedef'а named-tuple
  → CC-FAIL `unknown type name 'NovaTuple_Complex'`. **Уточнение:** «ранняя forward-декларация» из
  исходной формулировки НЕДОСТАТОЧНА (by-value член требует ПОЛНЫЙ тип, не forward-decl). **Фикс
  (точное зеркало NovaOpt VR-routing [M-153.2], D215):** `register_novares_decl` для late-by-value
  payload (named tuple / mono value-record) эмитит forward typedef рано + struct BODY/конструкторы в
  новую late-секцию `__NOVARES_VR_TYPEDEFS__` (после struct-bodies). Предикат — `is_late_emitted_value_payload`
  (§0 единый, переиспользован двумя NovaOpt-сайтами). Vec не ломался (element by-pointer). Verify:
  repro+detect172/u181 CC-FAIL→PASS, §0 GOLD 6 диров IDENTICAL, neg-control, unit. Блокировал complex.nv
  Ф.2a (РЕГРЕССИЯ) — миграция re-applied, complex `nova test` = PASS. **Cross-ref:** тактический unblock
  фрагментированного value-ABI; единая унификация (named-tuple/value-record/tuple → ОДИН путь) = **Plan
  172.4 Ф.3** — тогда этот late-routing станет кандидатом на удаление по построению.
- **[M-181-anon-record-in-ctor-arg-codegen]** ✅ **RESOLVED 2026-06-26** (`c724de7a`, ветка
  `plan-172-unified-type-engine`). Анонимный record-литерал в позиции аргумента конструктора/обёртки —
  `Ok({ tok, line, col })` / `Err({ why })` (json `Lexer.@next_token`/`Parser.new`) → `codegen error:
  anonymous record literal without spread not supported`. При прямом `return { .. }` codegen коэрсил по D55
  через `expected_record_type` (ставится вокруг тела fn, consumed анон-record-армом `emit_record_lit`);
  обёрнутый в `Ok(..)`, контекст = тип Result `NovaRes_<n>*` (не payload) → target-struct не найден.
  **Оказался ЛОКАЛЬНЫМ codegen target-propagation фиксом, НЕ полным RecordLit-резолвером** (U.4.5, 66%
  дивергенция канал↔legacy — sum/generic/value-контекст): contextual Ok/Err-арм `emit_call` уже несёт
  разрешённый payload-C-тип из канала (`novares_ok_err(&rt)`) → ставим
  `expected_record_type = struct_name_from_c_type(payload_c)` вокруг emit аргумента (зеркало D55, тип из
  канала, НЕ угадан). Byte-identical для не-анон-record аргументов (поле консультируется только
  анон-record-веткой + save/restore). Verify: repro CODEGEN-FAIL→PASS; §7.5 baseline-DELTA 20 диров
  FAIL-множества идентичны; §0 GOLD 45 .c / 8 диров sorted-line multiset-sha256 IDENTICAL; detect172
  `u181b_anon_record_in_ctor_arg_pos` 5 тестов + neg-control RUN-FAIL; unit types:: 51/0 +
  expected_record_type 1/0. Разблокировал json **ПАСТ** анон-record (упирается в downstream erasure-баг
  `as_array() -> Option[[]JsonValue]`, `[M-91.13]` — **НЕ регрессия**, оригинал уже падал `nova test`).
  Полный RecordLit-резолвер остаётся **Plan 172.1 U.4.5**.

- **[M-172.1-self-ref-slice-variant-erasure]** ✅ **RESOLVED 2026-06-26** (`98fa5c56`, ветка
  `plan-172-unified-type-engine`; закрывает json-блокирующую часть `[M-91.13]`). Self-referential sum-тип с
  payload-вариантом-срезом самого себя — `type Tree | Node([]Tree)` (json `JsonValue.Array([]JsonValue)`) →
  CC-FAIL: struct-поле `Nova_Vec____nova_int* _0` vs сигнатура `NovaOpt_Nova_Vec____Nova_Tree_p_p`. **Корень:**
  `emit_sum_type` лоуэрит payload-поля вариантов через `type_ref_to_c` ДО `sum_schemas.insert` → для self-ref
  `[]Tree` функция `is_generic_stub_c("Nova_Tree*")` возвращает true (Tree ещё не в `sum_schemas`) → элемент Vec
  эрейзится в `nova_int` (`resolved_array_to_c`). Non-self-ref `[]Other` и HashMap-вариант работали (Other
  зарегистрирован раньше; `resolved_named_to_c` без stub-проверки). **Фикс (ЛОКАЛЬНЫЙ, не U.1/U.4.5):** поле
  `being_defined_sum_types: HashSet` (set вокруг loop'а lowering'а payload-полей), консультируемое в
  `is_generic_stub_c` → тип-в-процессе-эмиссии concrete по построению. Generic-sum-путь бага не имеет
  (type-param payload → void*). Verify: repro CC-FAIL→PASS, §7.5 baseline-DELTA 18 sum-heavy диров FAIL-множества
  идентичны кроме 2 интенд-импактных, §0 GOLD 46 .c sorted-line (1 differ = process-noise eq-clause order,
  доказано same-binary), detect172 `u9113_self_ref_slice_variant_pos` 4 теста + neg-control, unit types:: 51/0.
  **json теперь КОМПИЛИРУЕТСЯ** (erasure ушёл). ⚠ json НЕ полностью зелёный: всплыли **2 отдельных
  пре-существующих RUN-FAIL** (`parse: object с полями` json.nv:852 + `parse: ошибка`) — object/HashMap-путь
  (`@parse_member(mut fields)` мутация / `.get` / sum-eq), НЕ array-path этого фикса. Priority: P2 (отдельный
  вопрос для full-green json Ф.2a). **UPD 2026-06-26:** корень `parse: object с полями` оказался **sum-eq**
  (НЕ мутация/get — те звучны) → закрыт `[M-172.1-option-eq-heap-aggregate-structural]` ниже; объект-тест
  ЗЕЛЁНЫЙ. Остаток full-green json: `into: array round-trip` (container-eq) + `parse: ошибка trailing content`
  (parser-логика) — см. follow-up'ы ниже.

- **[M-172.1-option-eq-heap-aggregate-structural]** ✅ **RESOLVED 2026-06-26 (sum-часть)** (`f53e32a9`, ветка
  `plan-172-unified-type-engine`). `Option[<heap sum>] ==` сравнивал УКАЗАТЕЛИ (`a.value == b.value`), не
  структуру — `Some(Str("a")) == Some(Str("a"))` = false (две аллокации). Корень: `nova_opt_eq_<T>` для
  `Nova_<X>*`-payload эмитился РАНО (до struct-body) → `is_pointer`-bail на идентичность = **use-before-ready**
  (фаза-корректность §0). Фикс (установить порядок + единый диспетчер): heap user-SUM payload → `nova_opt_eq`
  ПОЗДНО (`__NOVAOPT_VR_TYPEDEFS__`, после struct-bodies), где `emit_field_eq` дереференсит структурно;
  NPO-layout не меняется (eq-only). Попутно: `emit_field_eq` sum-рекурсия чинена для **record-style вариантов**
  (`V { a, b }` — позиционный `._0` → C error; теперь имена из `record_variant_field_order`; затрагивает и
  прямой sum==). Verify: repro CC/RUN→PASS, json object-тест ЗЕЛЁНЫЙ, §7.5 baseline-DELTA 22 диров 0 регрессий,
  detect172 `u172_option_sum_structural_eq_pos` 5 тестов + neg-control, unit types:: 51/0. **SCOPE = SUM-ONLY:**
  records/sums-с-record-полем — follow-up ниже.

- **[M-172.1-option-eq-record-structural]** ✅ RESOLVED 2026-06-26 (`917599e8`). `Option[<record>] ==` (и sum с
  record-ПОЛЕМ, и прямой `Rec==Rec`) сравнивал указатели → `Some(Pt{1,2})==Some(Pt{1,2})` false. **Оба слоя
  реализованы** (как и предсказано — один не закрывает):
  - **Слой 1 — proto-ordering ✅.** structural `nova_opt_eq` ФУНКЦИИ → новый буфер `novaopt_eq_fns_buf` + маркер
    `/*__NOVAOPT_EQ_FNS__*/`, splice'нутый ПОСЛЕ fn-forward-decls И `/*__MONO_FWD_DECLS__*/` (метод-прототипы
    видимы → нет implicit-decl `conflicting types Nova_<T>_method_equal`). Typedef остаётся рано (NPO single-pointer
    нужен только forward-typedef → циркулярность typedef↔proto разорвана). **Все** structural opt_eq (sum+record)
    унифицированы в этот буфер (sum переехал из `__NOVAOPT_VR_TYPEDEFS__`) — §0 единый источник per-type операций
    + «порядок эмиссии — одна дисциплина» (фаза-корректность). Verify: `nova_tests.types` (GrmPoint @equal —
    та самая регрессия broad-варианта) GREEN; .c — `nova_opt_eq_Nova_U172rKeyed_p` зовёт `Nova_U172rKeyed_method_equal`,
    proto эмитится РАНЬШЕ функции.
  - **Слой 2 — record-field structural recursion ✅.** `emit_field_eq` рекурсирует по `record_schemas` для
    record-без-@equal (records НЕ авто-`@equal`'ятся) — как для sum — вместо identity-fallback. Порядок полей —
    новый `record_field_order` (`record_schemas` — неупорядоченный HashMap → недетерминизм иначе). Empty-schema
    records (opaque builtin StringBuilder/WriteBuffer/...) исключены → identity (sound для handle). `early_gen`
    guard расширен на records (иначе `->field` в early opt-контексте = incomplete type).
  - **Предикат** `opt_payload_needs_structural_eq` расширен на `record_schemas` (non-empty минус
    generic/`____`/empty-builtin). Гипотеза «register до заполнения схемы» НЕ подтвердилась — record_schemas
    готов к моменту fn-pass'а (type-decl pass раньше); use-before-ready тут нет.
  **Verify (gate — не nova_tests):** detect172/`u172r_option_record_structural_eq_pos` PASS:1 (Option[record] +
  str-поле + record-с-@equal L1 + sum-с-record-полем + nested); §7.5 baseline-DELTA 25 диров clean=fixed FAIL-множества
  ИДЕНТИЧНЫ (0 регрессий) + 2-й батч 20 диров без новых CC-FAIL; unit types:: 51/0. **Остаток json:** container-eq
  (ниже) + trailing-content (parser).

- **[M-172.1-option-container-eq-structural]** ✅ RESOLVED 2026-06-26 (`f56cd7b7`) — **Vec/`[]T`-часть**.
  `Option[Vec[T]]` / Vec как sum-вариант-поле сравнивался по указателям (после record-фикса — рекурс в opaque
  `cap`/`data`/`len`, `data` по указателю) → `Some([1,2])==Some([1,2])` false. Fix: `emit_field_eq` для
  `Nova_Vec____<elem>*` зовёт MONO `Vec____<elem>_method_equal` (Nova-body `vec/protocols.nv`, element-wise), не
  erased generic (конфликт/SEGV) и не record-рекурс. **Граница &self/&mut:** mono-инстанциация — `&mut`, а
  emit_field_eq/register_novaopt_decl — `&self` → emit_field_eq пишет запрос в `pending_container_eq_monos`
  (RefCell, dedup `container_eq_requested`), `&mut` post-pass (в mono_worklist drain) инстанциирует через
  `vec_method_call`; каскад (Vec[Vec]/Vec[record]) — монотонно → терминирует. Предикат расширен на `Vec____`
  (late EQ_FNS splice → call резолвится); L2 record-branch исключает `Vec____`/`HashMap____` (контейнеры в
  record_schemas только для field-access). **Verify:** detect172 `u172c_option_container_eq_structural_pos` PASS:1
  (direct/Option[Vec]/sum-Vec-поле/Vec[str]/Vec[record]/nested [][]int); json `array round-trip` FIXED
  (`Array→Vec____Nova_JsonValue_p_method_equal`); §7.5 baseline-DELTA 25 диров 0 регрессий; unit types:: 51/0.
  **HashMap-часть → отдельный follow-up ниже** (нет `@equal` Nova-body).

- **[M-172.1-option-hashmap-eq-structural]** ✅ RESOLVED 2026-06-26 (`bd56022e`) — **завершает per-type-eq
  консолидацию** (sum→record→Vec→HashMap). `HashMap[K,V]` / `Set[T]` как Option-payload / sum-вариант-поле
  (`Object(HashMap[str,JsonValue])`) сравнивался по указателям → json `nested object round-trip` FAIL. Два слоя:
  (1) **`HashMap[K,V] @equal` Nova-body** (`hashmap.nv`) — order-independent: same `@len()` + `∀(k,v): match
  other.get(k) { Some(ov)=>v==ov; None=>false }`. Бонд НЕ нужен (K:Hash наследуется от типа, `==` на V
  диспатчится структурно — как `Vec[T] @equal`; `[K Hash+Equal, V Equal]` из старой формулировки оказался не
  обязателен). (2) **codegen routing:** `emit_field_eq` для `Nova_HashMap____*` → MONO
  `HashMap____<k>__<v>_method_equal` (обобщил Vec-ветку на оба контейнера); предикат разрешает `HashMap____`;
  drain зовёт **generalized `register_container_eq_mono`** (type-args из `generic_type_instance_info`, fn_decl из
  `generic_type_methods[base]` — работает для любого generic-контейнера). `Set[T]`=`HashMap[T,()]` покрыт
  автоматически. **Verify:** detect172 `u172h_option_hashmap_eq_structural_pos` PASS:1 (direct order-independent +
  Option[HashMap] + sum-поле + HashMap[str,sum] + HashMap[int,[]int]); **json `nested object round-trip` FIXED**
  (json.c: `Object→HashMap____nova_str__Nova_JsonValue_p_method_equal`, `Array→Vec mono`); §7.5 baseline-DELTA 25
  HashMap-heavy диров 0 регрессий (4=4); unit types:: 51/0. **json остаток = ТОЛЬКО `trailing-content`**
  (`[M-181-json-trailing-content]`, parser, отложен с sign-off владельца).

- **[M-181-json-trailing-content]** ✅ RESOLVED 2026-06-26 (`99077fbc`) — **json ПОЛНОСТЬЮ ЗЕЛЁНЫЙ**.
  `Json.parse("42 garbage")` возвращал `Err(UnexpectedChar)` вместо `Err(TrailingContent)`: существующая
  trailing-проверка (`Json.parse`: `cur==EofTok? Ok : TrailingContent`) не достигалась — value-завершающий
  префетч (`@advance()?` после scalar / закрывающего `]`/`}`) лексил trailing-garbage, а `next_token` на
  нераспознанном leading-char возвращал hard `Err(UnexpectedChar)` (пробрасывался `?` ДО проверки). **Fix**
  (parser-логика, .nv-only): `next_token` → токен-сентинел `BadTok(char)` вместо `Err` → value-парс завершается,
  top-level trailing-проверка классифицирует BadTok как TrailingContent (`_`-арм), а mid-structure BadTok →
  UnexpectedChar через catch-all'ы (таксономия сохранена). Покрывает scalar+array+object. Token не экспортируется
  → локально. **Verify:** std/encoding/json PASS:1 FAIL:0; plan91_13 (потребители) PASS:1; 0 регрессий. Завершает
  Plan 181 Ф.2a json end-to-end (все eq-блокеры sum/record/Vec/HashMap + parser).

- **[M-173-d194-perf-elision]** D194 §perf hot-path элизия (`Consumable[Never]`/`Cleanup[Never]` →
  strip shield/timeout/outcome/frame) **НИКОГДА НЕ БЫЛА РЕАЛИЗОВАНА** — де-риск Ф.2 (2 агента незав.)
  показал: ConsumeScope-ветка (emit_c.rs:19746-20031) эмитит полный frame-bearing путь БЕЗУСЛОВНО, нет
  effect-row inspection; спека D194 «Статус: ACTIVE / disasm-verified T2.9» дрейфанула; Plan 110:695
  сам числит это ❌-анти-паттерном. Ф.2 берёт PARITY (не регрессировать frame-bearing вывод). Генуинная
  элизия (ключ «sync-тело + cleanup `Fail[Never]` → прямой вызов без кадра») — самостоятельный
  перф-эффорт ПОСЛЕ Ф.2 (D194-спека уже приведена к факту в Ф.2.D194). Priority: P3 (перф, не корректность).

- **[M-174-lang-ffi]** Зонт lang/FFI-фич на едином type-engine ([174-lang-ffi-features.md](174-lang-ffi-features.md)). OPEN — под-планы в разной готовности (§1-таблица зонта).
- **[M-174.2-question-return-only]** `?` return-only. **Остаток ЗАКРЫТ 2026-07-06:** spec-closure D85 (auto-From блок, D165-ref фикс) + миграция `?→!!` (spec/prelude/examples) + Ф.B cross-carrier диагностики (`E_TRY_OPTION_IN_RESULT_FN`/`E_TRY_RESULT_IN_OPTION_FN`); ядро `E_TRY_IN_FAIL_FN` — 173 Ф.1. conformance neg 53/0.
- **[M-174.2-try-err-type-mismatch-hint]** (NEW, OPEN) `Result[T,E1]`-`?` при `E1≠E2` → hint `.map_err`. Отложено: наивный `E1≠E2` (base-name) даёт false-positive на легальном sum-extension widening (D85 «E' совместим с E»); корректная форма требует интеграции с sum-extension/assignability (зона 172.1). Priority: P3.
- **[M-174.1-parse-api]** (создан §3.7) Primitive parse API (D309). **Truncation-баг ЗАКРЫТ 2026-07-06** (`emit_parse_range_check`, sub-width range-check; 20 pos-тестов; load-bearing доказан против baseline). `@parse_int→Result` поглощён 177 Ф.2b.
- **[M-174.1-parse-engine-structural]** (OPEN; ПЕРЕОФОРМЛЕН 2026-07-08 под conversion-on-source канон) Живой остаток 174.1 после волны `to_*`: удаление codegen-хардкода скалярных `T.try_from(str)` (36 call-site'ов `int.try_from(s)` и т.п. всё ещё идут через перехват `emit_c.rs` `:~32888` — миграция на `s.to_int()` + removal перехвата) + float-канон (§4.1: no-trim, locale, f32 strtof, full-consume) + `char.try_from`-типизированные ошибки. САМА поверхность СДЕЛАНА 2026-07-08 (str @to_int/@to_i64/@to_u64/@to_u32/@to_u8 c range-check + @to_f64; str.parse_int ретрактирован). Gate: координация 172.1-hardcode-зона × 177 Ф.2b. Priority: P2.
- **[M-174.1-vec-method-chain-elem-erasure]** (NEW 2026-07-08, OPEN) Facade-`[]T`-метод, чейненный НА РЕЗУЛЬТАТЕ fn-вызова/slice-выражения (`slice(b,0,i).to_str_unchecked()`, `buf.into().to_str()`), теряет конкретный элемент → `[E7320] no method on type Vec` / P67. Обход: явный `[]u8`-локал перед вызовом (промаркированные сайты в std/http wire, servernet, transport, net/addr|error, encoding/url, examples). Priority: P3.
- **[M-174.1-into-str-unchecked-legacy-array-codegen-gap]** (NEW 2026-07-08, OPEN) `[]u8 consume @into_str_unchecked()` (zero-copy steal) на legacy-NovaArray пути (`#no_prelude`-юниты: string_builder `@buf`) эмитит C-тело с `int`-возвратом → CC-FAIL. Обход: StringBuilder @into_str использует copy-вариант `@to_str_unchecked()` (корректность не страдает; теряется zero-copy-reuse). Priority: P3.
- **[M-dce-seed-name-list]** (NEW 2026-07-08 по ревью владельца, OPEN) Interp-DCE seed-лист в `lints.rs` (`collect_used_names`, ExprKind::InterpolatedStr arm) — hardcode-класс: компилятор знает std-имена руками (`StringBuilder/append/into_str/cap/display/debug/from/from_debug/to_str`). Уже дважды протухал (as_str→into_str: тело into_str выкидывалось DCE при живом вызове → implicit-int CC-FAIL в ЛЮБОМ fn-main CU с `${…}`; with_capacity). Правильный канал: seed из emission-фактов desugar'а (reachability-корни/аннотации), не рукописный список. Адрес: 185/172.13-хвост. Priority: P3.
- **[M-spawn-ctx-module-const-capture]** (NEW 2026-07-08, OPEN) Spawn-ctx захватывает МОДУЛЬНЫЕ const по голому имени (`_ctx->MESSAGE = &MESSAGE;`), тогда как const эмитится как `Nova_const_<module>_MESSAGE` → undeclared identifier CC-FAIL. Репро: examples/net/echo_client.nv (spawn-тело читает module-const). Не связан с 174.1 (вскрыт гейтом); examples не гейтятся дефолтом. Priority: P3.
- **[M-174.1-nested-match-err-variant-after-loop]** (NEW 2026-07-08, OPEN) Вложенный `match r { Ok=>.., Err(e) => match e {..} }` на `Result[T, UserEnum]`, когда callee содержит for-цикл с ранним `return Err(...)`, иногда неверно резолвит вариант (репро: свежий минимальный тип+fn; плоский `Err(Variant)=>`-match корректен). Обойдено плоскими match'ами в фикстурах 174.1 (plan91_fe2/neg, conformance). Priority: P2 (корректностный класс).
- **[M-174.5-pointer-ops-methods]** (OPEN) Pointer-ops методы + write-cap fix + `unsafe T`→`uninit T`. **§7.7-оценка 2026-07-06:** write-cap-баг ЖИВ (spec §11a `02-types.md:8522` голый `*unsafe T` writable; checker `.write()` минует `pointee_is_writable` `types/mod.rs:13847`; codegen `emit_c.rs:27263`/`:40347`). Гейт РЕАЛЬНЫЙ — фикс требует amend `02-types.md` (D216 write-table + D352 + Ф.0 rename ~90 вхождений) = зона 172, «не в одиночку». Поглощает `[M-138.5-unsafe-ptr-write-cap]`, `[M-118.4-typed-ro-write-error]`. Priority: P2.

- **[M-172.5-inout-ref]** ✅ RETIRED (superseded by Plan 184, 2026-07-06/08) — `mut ref` param-mode machinery (`ParamRefMode`, call-site `ref x` marker) described here was RETRACTED wholesale by Plan 184 (`ref T` → ограниченный тип; in-out теперь просто `mut x T`, Р10). Historical only — see [184-ref-type-revision.md](184-ref-type-revision.md) for the current landed design.
- **[M-172.5-chain-gating-ro-at]** ✅ CLOSED 2026-07-10 — the marker's OWN description was stale (referenced retracted `mut ref` machinery), but the underlying soundness hole it named was REAL and sharpened by D326-Plan184 Р7 (`-> @` from a non-mut method is now a genuine `ref Self`, not a copy — `c.peek().bump()` empirically left `c.x == 1`, not the harmless-no-op the old note assumed). Fixed in `types/mod.rs` `consume_walk_expr` (new Call-receiver branch alongside `lvalue_root_ident`, arity-aware `mut_methods_arity`/`ro_methods_arity` + `recv_returning` self-return gate to avoid false positives) → `E_RECEIVER_BINDING_NOT_MUT`. Spec: D33 amend §«Fluent `-> @` chain-receiver mutability gate» (02-types.md). Tests: `spec_tests/conformance/d326_chain_gating_ro_at.nv` (+neg). Gates: conformance 90/0, std/ + nova_tests fluent/chain sample — identical FAIL-sets vs clean baseline (0 regressions).
- **[M-172.5-generic-mut-ref-codegen]** ✅ RETIRED (moot, 2026-07-10) — described `fn f[T](mut ref x T)` codegen; that param-form was fully retracted by Plan 184 (no `mut ref` in parameter position anymore — parser rejects it, `E_REF_PARAM_FORM_REMOVED`). The MODERN equivalent concern — generic `fn f[T](mut x T)` mono-codegen when `T` binds to a VALUE type — is tracked separately as `[M-184-value-mut-mode-overload-abi]` (Plan 184 заход-6 follow-up; overload-mode axis + value-type ABI interaction, not this marker's scope).

- **[M-179-brotli-conditional-link]** ✅ МЕХАНИЗМ (справка). Линковка brotli **условная по использованию** (owner-требование; libuv остался mandatory/always): `test_runner.rs::c_file_uses_brotli` сканирует генерённый `.c` на **call-site** `brotli_decode(` (фильтруя `static …);`/`static …{` — forward-decl и definition-header, т.к. std-fn'ы эмитятся даже мёртвыми); только тогда `brotli_shim.c` компилируется с `-DNOVA_USE_BROTLI` и `libbrotlidec.lib` попадает в линк (все 3 toolchain-арма: Clang/MSVC/GCC). Без vendor-lib шим = Q11-заглушки → `UnsupportedMethod`, не link-error. Диагностика: `NOVA_DEBUG_BROTLI_LINK=1` (stderr: `uses_brotli=… → LINK/NO`). Доказано: gzip-only CU → NO lib; brotli/http-br → LINK.
- **[M-179-brotli-reader-streaming]** (OPEN) Streaming `BrotliReader` (`consume value`, D133/D337 R2 — единственный consume-кодер модуля). C-примитивы шима уже инкрементальные (`nova_brotli_dec_feed`/`pull`/`done`/`needs_input`); остаётся тонкая Nova-обёртка + consume-neg-тесты (`EXPECT_COMPILE_ERROR`: не-consume / double-consume). Отложено сознательно: Ф.2-deliverable = one-shot `brotli_decode`; единственный потребитель `br` (http auto-decompress) использует one-shot, симметрично gzip/deflate-веткам `finalize_response`. Priority: P3.
- **[M-179-brotli-unix-lib]** (OPEN) `libbrotlidec.a` для Linux/macOS не вендорен (сборка выполнялась на Windows-хосте; `detect_brotli` ищет `.a` в тех же двух путях). До vendor'а на этих хостах brotli = Q11-заглушки. Priority: P3.
- **[M-178-d78-rev1-module-decls]** (OPEN, вскрыто переносом тестов 179 Ф.2) Вложенные folder-модули std/http декларируют retired rev-1 форму `module std.http.X` вместо канона rev-3 `http.X` ([M-D78-strict-removal] 2026-06-01 → hard error `E_D78_MODULE_PATH_MISMATCH`, но латентно: всплывает ТОЛЬКО когда папка становится test-entry). `std/http/client/` мигрирован на `module http.client` (4 файла; импорты `std.http.client` работают — прецедент `encoding.compress`, все потребители зелёные). Остальные: `std/http/{server,servernet,serdejson,transport}` — мигрировать при первом же in-module тесте (или разом). Priority: P3.

- **[M-183-old-net-removal-after-182]** (OPEN) Физическое удаление старого net-слоя
  (`compiler-codegen/nova_rt/net.c`/`net.h`, `std/net/*.nv` (`ffi.nv`/`addr.nv`/`tcp.nv`/`udp.nv`/
  `dns.nv`/`mock.nv`/`effect.nv`), все `NovaRt_*_method_*`/`NovaRt_*_static_*` — ОТЛОЖЕНО
  до санации `nova_tests` (Plan 182), т.к. `nova_tests/plan83_12/*`, `nova_tests/plan91_12/*`,
  `plan91_15/*`, `plan91_16/*`, `nova_tests/plan178/net_byte_surface_mock.nv` остаются на старом
  слое (по директиве 183 Ф.3 — эти тесты уходят в санацию 182, не мигрируются здесь).
  Все живые потребители (`std/http/transport/real.nv`, `std/http/servernet/servernet.nv`,
  `examples/net/*`, `nova_tests/http_transport`, `nova_tests/http_servernet`) уже на `std/net2`
  (Ф.3, этот заход) — `std/net/*.nv` несут `// DEPRECATED` баннер. Удаление-гейт (проверить ПЕРЕД
  удалением): `grep -rl "import std\.net\." nova_tests std examples` = только
  plan83_12/91_12/91_15/91_16/plan178 (после их санации/удаления в 182 — пусто);
  `grep -c "NovaRt_.*_method_\|NovaRt_.*_static_" compiler-codegen/nova_rt/net.c` (файл целиком уходит);
  после удаления — рассмотреть namespace-ренейм `net2` → `net` (тоже отложено, см. план 183 §5).
  Priority: M (гейтовано на Plan 182).

- **[M-183-nova-build-consume-effect-close-ice]** (NEW, OPEN) `nova build` (fn-`main`-rooted
  compile, в отличие от `nova test`/`test-build`, которые компилируют `test { }`-блоки) паникует
  (ICE) на ЛЮБОЙ программе, где `mut x = match SomeEffect.op(...) { Ok(v) => v, Err(_) => panic(...) }`
  связывает переменную consume-обязательного типа, возвращённого через effect-dispatch
  (`TcpListener.bind` через `Net`), а затем эту переменную потребляет `.close()`:
  `internal error … [P67-LEGACY] method call `.close` return type unknown — checker must
  annotate; obj_ty="" obj=Ident(lst)`. Репродуцировано МИНИМАЛЬНО (без spawn/supervised,
  без net2-специфики): идентичный ICE с **старым** `std.net`/`mock_net()` тоже — это НЕ регрессия
  Plan 183 и не специфика `std.net2`, а общий разрыв между `nova build`- и `nova test`-путями
  тайпчека consume-результатов effect-операций (родственно `[M-172-nova-int-fallback-audit]`/
  U.4 классу «checker резолвит по-разному в разных entry-points»). Обнаружено при миграции
  `examples/net/{echo_client,echo_server}.nv` на `std.net2` (Ф.3): оба файла типобезопасны
  (`nova check` → PASS) и логически корректны (тот же паттерн accept/read/write/close зелёный
  в `nova_tests/http_servernet` и `plan91_12/net_v2_tcp_echo_slow` через `nova test`), но
  `nova build examples/net/echo_server.nv` падает ICE независимо от структуры (voпробовано:
  с/без `supervised`, прямой вызов vs через helper-fn). Затрагивает ЛЮБОЙ example/CLI-программу,
  использующую `Net`(или любой другой) эффект с consume-типом результата — не только net.
  Priority: P1 (блокирует `nova build` для net-примеров и, вероятно, шире — любые consume-типы
  через effect-dispatch вне test-контекста).

- **[M-183-net2-loop-affinity-cross-thread-op]** (✅ CLOSED 2026-07-10, волна Plan 116
  Ф.3 / [M-116-handshake-socket-deadlock]) Loop-affinity-контракт net.c: uv-handle
  пришпилен к loop'у, на котором создан (`nova_current_loop()` в bind/connect/accept),
  а libuv-loop'ы НЕ thread-safe — единственный cross-thread-safe вход `uv_async_send`.
  «ОСТАТОЧНЫЙ класс» из исходной записи (волокно УКРАДЕНО work-stealing'ом между
  park'ами → следующий оп на handle с нового worker'а = конкурентная мутация чужого
  loop'а → completion теряется → вечная парковка) МАТЕРИАЛИЗОВАЛСЯ как runtime-дедлок
  TLS-handshake smoke (~1/300; плотное write→park→read на одном сокете = ровно то
  окно миграции, которое stress_test не покрывал). **Фикс — предсказанный этой же
  записью маршалинг issue-стороны:** `nova_loop_defer_call` (eventloop.h/.c +
  runtime.c; generic-обобщение `nova_loop_defer_close`: per-loop mutex-очередь
  `NovaDeferredCallQueue` у main + каждого worker'а, drain в тех же async-callback'ах)
  + в net.c каждая issue-точка (tcp read/write/accept/shutdown, udp send/recv)
  ветвится: same-thread = прямой вызов (байт-в-байт прежнее поведение, ноль
  оверхеда), cross-thread = маршалинг `_deferred`-обёртки на owning-thread — ТОЛЬКО
  она публикует completion-latch и будит (урок: unconditional latch+wake на
  same-thread пути = reentrant self-wake ДО собственной парковки → гонка с
  gopark/goready; задокументировано в net.c). Верификация: TLS handshake smoke
  0 зависаний на 720+ прогонах (8×90) против ~1/300 до; регресс —
  `std/net/pingpong_test.nv` (alternating write→read, 2 теста). Паттерн
  «создавай сокет внутри волокна» остаётся рекомендацией (bind/connect
  по-прежнему пиннят к current loop), но узкое окно миграции закрыто.

- **[M-net-cu-segv-under-cpu-pressure]** (NEW, OPEN, 2026-07-10, волна 116 Ф.3)
  std/net folder-CU exe (37 тестов) редко (~2-5%) падает SIGSEGV (exit 139), когда
  4 КОПИИ exe гоняются параллельно на 16-ядерной машине (CPU-давление сжимает/
  растягивает тайминги). Одиночные прогоны стабильны (0 segv на 300+). ДОКАЗАНО
  pre-existing и НЕ регрессия волны 116: baseline-бинарь (c7a184807, до фиксов
  defer_call/deref) под той же нагрузкой падает ЧАЩЕ (6/120 против 3/120 у
  исправленного). Точка падения плавает от прогона к прогону (после udp-тестов /
  mock-тестов / error-тестов — без привязки к конкретному тесту) → класс
  GC/fiber-arena/runtime-shutdown race, проявляющийся при вытеснении. Репро:
  `for w in 1..4 parallel: for i in 1..30: timeout 60 <std-net-CU>.exe` —
  считать exit 139. Не путать с [M-183-net2-loop-affinity-cross-thread-op]
  (тот CLOSED: там вечная парковка, тут crash). Priority: P2 (одиночные
  прогоны и `nova test` стабильны; всплывает только при параллельном стрессе).

- **[M-183-int-to-str-module-method-collision]** (NEW, OPEN) Компиляторный дефект
  разрешения методов: внутри модуля, определяющего СВОЙ `X @to_str()` (здесь
  `std.net2`: `NetError @to_str`), вызов `mibps.to_str()` на **int**-receiver'е
  резолвится в метод модуля (`Nova_NetError_method_to_str(mibps)` в эмитированном C) —
  int передаётся как указатель на enum → deref малого целого → SEGV. Пойман Ф.4-стрессом
  (FaultAddress = значению int'а: mibps=12 → 0xC). `${...}`-интерполяция эмитится
  корректно (StringBuilder int-append) — использована как обход в
  `std/net2/stress_test.nv` (см. NOTE там). Класс: same-module method-name collision
  побеждает builtin-метод примитива; вероятно касается любого имени, совпадающего
  с builtin (`to_str`, `len`, ...) в модулях с одноимёнными методами на своих типах.
  Priority: P1 (тихая генерация некорректного кода без диагностики).
  **Статус на закрытие Плана 183 Ф.5 (2026-07-06): в работе** (вне периметра плана 183 —
  трекается отдельно как компиляторный дефект; обход в `stress_test.nv` остаётся, пока
  не закрыт).

- **[M-183-gc-vec-value-heap-tracing]** (NEW, OPEN) GC-трассировка `Vec[value-record с
  heap-полем]` (здесь `Vec[SocketAddr]`, где `SocketAddr` = `value { priv raw []u8 }`) не
  переживает GC, если (a) `Vec` пересекает effect-vtable как `Ok`-payload, ИЛИ (b)
  извлекается через generic `_must[T]` (type-erasure). Митигация в `std/net2/dns.nv`:
  `resolve()` строит `[]SocketAddr` ВНЕ vtable (через vtable идёт только скаляр-пара
  `(base, count)`); тесты используют прямой `match`, не generic `_must`, и читают значения
  сразу после получения. Корень — компиляторная GC-трассировка сквозь
  `Vec[value-struct-with-heap]`/generic-erasure; инлайн `[20]u8`-репрезентация убрала бы
  проблему, но литерал-повтор `[0; N]` — известный отдельный gap (blocks crypto-тесты
  тоже). Priority: P2 (митигировано во всех текущих потребителях, но ловушка для будущего
  кода с тем же паттерном).

- **[M-183-unwrap-typed-error]** (NEW, OPEN) Pre-existing gap: `Result[_, XError].unwrap()`
  эмитит `Nova_Fail_fail(str)` и не компилируется для ЛЮБОГО типизированного `XError`
  (репродуцировано минимально даже на `parse_int().unwrap()`, не net2-специфика). Все
  net2-тесты обходят через `match`/собственный `_must`-хелпер. Priority: P2 (широкий
  compiler-gap за пределами net; net2 — просто первое место, где это стало заметно
  массово из-за объёма нового test-кода).

- **[M-183-resize-inference-inferred-vec]** (NEW, OPEN) `mut b = []u8.new()` (тип выведен
  инференсом, без явной аннотации) НЕ персистит последующий `resize` — эффект теряется,
  `len` остаётся 0 → буфер трактуется как `null_buf` дальше по цепочке (порча данных, не
  compile-error). Обязательная аннотация `mut b []u8 = []u8.new()` обходит дефект; принята
  как конвенция во ВСЕХ net2-буферах (addr.nv/tcp.nv/udp.nv/dns.nv/тесты). Priority: P3
  (обход прост и уже применён; компиляторный gap инференса `mut`-без-аннотации + method-эффект
  на выведенном типе).
- ✅ **FIXED [M-172-errdefer-okdefer-dead-surface]** (Plan 173 Ф.1, 2026-07-03) — все три слоя закрыты.
  `errdefer`/`okdefer`/`defer |result|` ретракнуты (D189, hard cutover); парсер реджектит их
  tombstone-хинтом `[D189-removed-*]` (`parser/mod.rs:10052-10090`). **(1) USER-FACING БАГ (P1) —

- ✅ **FIXED [M-vec-access-e7320-as-bytes-str]** (2026-07-06 найден / 2026-07-07 исправлен) — `nova
  test --full std/collections/vec` давал CODEGEN-FAIL юнита `vec/access`: `[E7320] no field or
  method as_bytes on type str` со спанами access.nv:89/99/107, при этом в vec НЕТ ни одного
  вызова `as_bytes`. Корень (двойной): (1) частичный prelude `#prelude(core, runtime, collections,
  protocols)` не привозит `std.runtime.string`/`std.runtime.char` — `#no_prelude`-хелперы
  string_builder.nv/write_buffer.nv (притянуты пакетом `collections`) зовут
  `str.@as_bytes()`/`char.try_from()`, рассчитывая на то, что consumer уже привёз ПОЛНЫЙ prelude;
  под частичным prelude(collections) эти символы никогда не привозились → E7320 в ЛЮБОМ CU с
  частичным prelude(collections). (2) раннер рендерил кросс-файловые диагностики через
  `SrcResolver::Single`, игнорирующий `span.file_id` (`test_runner.rs` → `diag.rs`), поэтому ошибка
  печаталась на ложных координатах entry-файла — отсюда впечатление «фантомного as_bytes на
  access.nv». Фикс 1 (вариант A): `std/prelude/collections.nv` теперь явно довозит
  `std.runtime.string`, `std.runtime.char` и `std.prelude.errors.{CharTryFromError,
  TryFromCharError, ReadBufferError, UnexpectedEnd}` (error-типы нужны on hop deeper —
  `char.try_from`/`ReadBuffer.@read_*` конструируют их) плюс `std.runtime.defaults` (u8/int/…
  `@compare`, нужен ещё on hop deeper — `Vec[T Compare] @compare`, транзитивно притянутый через
  `std.runtime.string` → `std.collections.vec.{Vec}`, иначе элемент-wise compare для `Vec[u8]`
  молча мис-диспатчился на `str`'s compare). Фикс 2 (вариант D):
  `Diagnostic::render_with_map(&SourceMap)` — `codegen_to_c` (test_runner.rs) теперь строит
  `SourceMap` из `module.peer_files` (заполнен `resolve_imports_inline_ex`, строго по возрастанию
  `file_id`; non-entry файлы перечитываются с диска для diagnostic-рендера) и рендерит через него
  все диагностики ПОСЛЕ разрешения импортов (checker/lints/verify/const-fn errors) — cross-file
  спаны показывают ИСТИННЫЙ файл. Регресс: `spec_tests/conformance/partial_prelude/
  d371_partial_prelude_collections.nv` (изолированный под-модуль — `#prelude(...)` целый-CU
  атрибут нельзя воткнуть одним peer'ом в разделяемый 182-файловый `spec_tests.conformance`, ломает
  folder-module peer detection для остальных 181). Гейты: `vec --full` 2/2 PASS (access теперь
  проходит); conformance --positive --compile-error 54/0 (53 база + 1 новый); nova_tests/buffers
  2/2 PASS. Durable-семейство (эта же категория дефекта, шире E7320) — заведено отдельным маркером
  [M-partial-prelude-primitive-method-registry] ниже.

- **[M-partial-prelude-primitive-method-registry]** (ДОПОЛНЕНО 2026-07-07 бисектом: standalone-CU read_buffer/string_builder/write_buffer в std/runtime битые этим классом — char.from, str.from_bytes_unchecked_steal, s.bytes()-fallback в field-доступ `no member in nova_str`; pre-existing на всех точках) (2026-07-07, P2, Wave: фикс-очередь §4а /
  план 172-семья) — durable-семейство, вариант C из разведки [M-vec-access-e7320-as-bytes-str]:
  сейчас «метод примитива известен компилятору» ЖЁСТКО связано с «файл, объявляющий этот метод,
  физически слит в текущий CU» (через `#prelude`/`import`). Каждый раз, когда `#no_prelude`
  runtime-файл (char.nv, defaults.nv, string/core.nv, read_buffer.nv, write_buffer.nv,
  string_builder.nv) использует чужой примитив-метод/error-тип, а не декларирует его как
  собственную зависимость, любой ЧАСТИЧНЫЙ prelude, который его притягивает без «попутчика» из
  полного prelude, ловит либо E7320/undeclared-identifier (найдено и залатано точечно в этом
  заходе — as_bytes, char.try_from, CharTryFromError/TryFromCharError, ReadBufferError/
  UnexpectedEnd, u8/int `@compare` из defaults.nv), либо ТИХИЙ мис-диспатч на C-уровне без
  Nova-диагностики (Vec[T Compare] на T=u8 при отсутствии `std.runtime.defaults` в CU молча звал
  `str`'s compare вместо `u8`'s — не репортилось как ошибка чекером, только как C-compile
  type-mismatch). Точечные патчи в `std/prelude/collections.nv` закрыли ИМЕННО тот набор символов,
  который нужен `collections`-facade; тот же паттерн латентен для ЛЮБОЙ другой partial-prelude
  комбинации (core-only, runtime-only, …) и для ЛЮБОГО другого `#no_prelude` runtime-файла, чьи
  транзитивные зависимости ещё не перечислены. Durable-фикс: зарегистрировать сигнатуры методов
  примитивов (и error-типы, которые они конструируют) как lang-item в чекере — развязать «метод
  известен» от «файл слит в CU», вместо ручного заведения explicit-import на каждый найденный hop.
  Смежная находка того же захода: `std/unicode/collate.nv:236/245` (`cur_consumed`/`prev_ccc`
  undefined identifier) — пред-существующий дефект от sweep-коммита `af4df4bdf`, НЕ относится к
  этому маркеру, но блокирует `nova_tests/strings` (str_builder_consume_test и др., через общий
  folder-module `nova_tests.strings`) от полного прогона; отдельный маркер не заведён — просто
  зафиксировано здесь как побочная находка.

- **[M-test-runner-shared-temp-collision]** (2026-07-07, P2, Wave: фикс-очередь §4а после разведки
  E7320) — **✅ FIXED** (2026-07-07, worktree `nova-runner`/`runner-fixes`). параллельные прогоны
  `nova test` из разных процессов бьются на общем `%TEMP%\nova_tests\t-<hash>` (артефакты одного
  CU перетираются) → ложные единичные FAIL (наблюдалось 52/1 на эталонных при четырёх фоновых
  агентах; с приватным TEMP стабильно 53/0; впервые диагностировано sweep-агентом). Фикс:
  `default_tmp_dir()` (nova-cli/src/main.rs) теперь строит корень как `nova_tests-<PID>` (было
  константное `nova_tests`) — `std::process::id()` уникален и для параллельных процессов, и для
  каждого нового последовательного запуска `nova.exe` (новый процесс = новый PID); детерминизм
  ВНУТРИ одного прогона сохранён (`test_subdir`'s per-display hash не менялся). Приёмка:
  2 параллельных фоновых `nova test nova_tests/buffers` без приватного TEMP, ×3 повтора — все
  6 PASS (было бы флаком до фикса). Проверено также `duplicate_hashable_protocol.nv` (nova_tests/
  plan62) флапавшая PASS/RUN-FAIL — **флак сохранился и после фикса, включая single-process
  sequential прогоны без единого элемента параллелизма/temp-коллизии** → диагноз этого конкретного
  флака НЕ temp-race (вероятно отдельный дефект — RUN-FAIL обрывается на 4-й из 90 проверок,
  похоже на crash/GC-race в рантайме теста); отдельный маркер не заведён, зафиксировано здесь как
  побочная находка для дальнейшего разбора.

- **[M-http-props-mut-chain-argpos-value-ptr-mismatch]** (2026-07-07, P1, Wave: 184 Ф.2 —
  приёмочный тест) — **ЗАКРЫТ 2026-07-07 (Plan 184 Ф.2, заход 2).** Результат беглой
  `-> @`-цепочки value-типа, использованный напрямую аргументом вызова (`take(r.b(5))`),
  давал CC-FAIL: `passing 'NovaValue_X *' to parameter of incompatible type 'NovaValue_X'`.
  Фикс (Р5/Р7): на free-fn call-site резолвится C-тип параметра callee, и беглый value-ptr
  (`NovaValue_X*` = ref Self) разыменовывается при встрече с by-value параметром
  (auto-conversion `ref T -> T`); `mut x T` in-out параметр (C `NovaValue_X*`) получает
  указатель без изменений. Guard: приёмочный `nova_tests/inout_ref/p184_defect_a_argpos.nv`.
- **[M-http-props-mut-chain-stmt-value-copy-loss]** (2026-07-07, **P1 ТИХАЯ ПОРЧА**, Wave:
  184 Ф.2 — приёмочный тест) — **ЗАКРЫТ 2026-07-07 (Plan 184 Ф.2, заход 2).** Цепочка ДВУХ
  mut-сеттеров на value-типе как statement МОЛЧА ТЕРЯЛА ОБЕ мутации. Корень: root-temp
  hoist chain-norm (`let _chain_root = <root>`) КОПИРУЕТ value-типизированный корень (теряет
  идентичность приёмника). Фикс (Р7): в chain-norm протянут `resolved_types`, hoist
  ПРОПУСКАЕТСЯ для value-типизированных корней — сырая вложенная форма нитит `ref Self`
  корректно; глубина ≥ 3 дополнительно потребовала `prepare_method_recv` пропускать уже-
  `ref Self` указатель приёмника вместо materialize-and-address. Guard: приёмочный
  `nova_tests/inout_ref/p184_defect_b_chain.nv` (depth-2 и depth-3). Канон «один сеттер на
  statement» в std/http больше не обязателен (беглые value-цепочки теперь корректны).
- **[M-http-server-module-path-legacy]** (2026-07-07, P2, Wave: волна миграций
  D410/http-хвост) — server.nv/servernet.nv остались на legacy `module std.http.server`
  (client.nv уже на rev-3 `http.server`-форме) → beside-module тест server_test.nv как
  CLI-root упирается в E_D78_MODULE_PATH_MISMATCH (pre-existing, найден агентом http-props).
- **[M-http-builders-second-pass]** (2026-07-07, P3, Wave: волна миграций, http-хвост) —
  копирующие строители вне списка первой волны: ServeMux.@handle/@get/@post/@put/@delete/
  @patch/@not_found, RequestBuilder, MockResponse — привести к mut-свойствам по D117 AMEND-2
  (Body.@with_limit и HttpError.@with_url легальны: consume-линейный и полная замена).
- **[M-record-elem-vec-bare-ctor-miscompile]** (2026-07-07, **P1 ТИХАЯ ПОРЧА КУЧИ**, Wave:
  D410/http-хвост) — ✅ **FIXED 2026-07-07 (ветка record-elem-fix).** `mut out = []Rec.new();
  out.push({поля})` с RECORD-элементом мискомпилировался. **Корень:** D38 `[]T.new()` /
  `[]T.with_capacity()` мост (emit_c.rs `ExprKind::Member`-арм) для RECORD/SUM/named-tuple
  элемента отдавал `nova_array_new_nova_int` — `arr_suffix` проваливался в `_ => "nova_int"`
  (эрейз в int-слот `NovaArray`), тогда как чекер и type-side (`resolved_array_to_c`) типизировали
  биндинг как `Nova_Vec____NovaValue_Rec*`. Скаляры не задеты: `NovaArray_<prim>` layout- и
  slot-совместим с `Nova_Vec____<prim>`; record-слот (`NovaValue_Rec`) — нет → `NovaOpt_nova_int`
  vs `NovaOpt_NovaValue_Rec` на `.get()` (громкий CC-FAIL) ИЛИ тихая truncation value-record'а в
  том же CU (named-литерал). **Фикс (§4а, не обход):** и emit-side, и ОБА R3-зеркала
  (`infer_expr_c_type` &self/&mut) для КОМПОЗИТНОГО элемента (`!is_primitive_array_elem_c`)
  маршрутизируют `[]T.new()`/`.with_capacity(n)` через ту же Vec-машинерию, что `of`/`from`
  (turbofish `Vec[T].new()` / `.new().cap(n)`), давая правильный `Nova_Vec____<elem>` mono;
  скаляры/эрейз-typevar byte-identical на legacy-пути. Родственный **[M-record-tuple-vec-empty-
  literal-miscompile]** (`mut out [](Rec,Rec) = []`) закрыт тем же корнем. Попутно — анон-форма
  `push({поля})` (пре-существующий gap [M-181]-класса, воспроизводился и на голом
  `Vec[Rec].new()`): generic instance-method dispatch ставит `expected_record_type` из
  резолвнутого param-C-типа вокруг emit анон-record-аргумента (D55; узко — только анон-RecordLit
  с param в `record_schemas`). **Обход снят:** 12 record-vec + 1 record-tuple места в std/http
  (header/client/mock/cookie/mime/response_ext/server) возвращены на `[]T.new()` / `[](A,B)=[]`,
  комментарии-якоря удалены. **Гейты:** conformance --positive --compile-error PASS + 4 новых
  D38-юнита (named/anon/cap/record-tuple, проверка ПОСЛЕ 2-го push); std/http server/client/neg,
  vec, json_test — pass; широкая дельта против main-бинаря — 0 регрессий.


- **[M-f64-try-parse-to-parse-f64]** ✅ **FIXED** (2026-07-07) — `f64.try_parse(s) ->
  Option[f64]` нарушал R1/R3/R4 (D325: Option вместо Result; try_ без infallible-сиблинга) и
  §3 (компиляторный builtin, хардкод в emit_c.rs поверх `nova_str_to_f64`). Снесено: f64-арм
  вырезан из ВСЕХ мест `try_parse`-таблицы (Path-form value-emission + 2 дубликата
  return-type-inference + `infer_static_method_ret` exclusion-list) — `f64.try_parse(...)`
  теперь честный `[E_UNKNOWN_STATIC_METHOD]` (fall-through guard [M-154.1], verified). `f32`
  НЕ тронут (вне скоупа фикса — try_parse для f32 остался).
  Замена: `f64.parse(s str) -> Result[f64, ParseFloatError]`
  (`std/runtime/string/parse.nv`) — **ЦЕЛИКОМ Nova-body, БЕЗ компиляторного знания** (§3-
  коррекция владельца 2026-07-07 после первого захода с приватным `__parse_f64_opt`
  builtin-триггером — тот подход был "хардкод хардкодом", отклонён и снесён обратно).
  Финальная форма: тонкий out-param C-шим `nova_str_parse_f64(nova_str s, double* out) ->
  nova_bool` в `nova_rt/conv.h` (оборачивает существующий `nova_str_to_f64`, D407/net2-стиль
  bool+out-указатель, прецедент `out_err *mut int` в net2/tcp.nv) + ОБЫЧНАЯ `extern "C" fn
  nova_str_parse_f64(s str, out *mut f64) -> bool` FFI-декларация (D282) в parse.nv + чистое
  Nova-тело (`mut v = 0.0; if unsafe { nova_str_parse_f64(s, &v) } { Ok(v) } else {
  Err(Invalid) }`, auto-address `&v` на mut-локал). Компилятор резолвит вызов ОБЫЧНЫМ FFI-путём
  — ни одного нового имени parse-семейства в чекере/кодогене. Вызовов мигрировано 2:
  json.nv:483 (`match f64.try_parse(text) { Some.. None.. }` → `match f64.parse(text) { Ok..
  Err(_).. }`), _experimental complex.nv:368 (аналогично `parse_f64_or_err`).
  Спека: 02-types.md примеры type-set блока `T.try_parse(...)` → `T.parse(...)` (13912/6456/
  строка "Reuse через семейства") + однострочный R3-амендмент с датой (первое лицо).
  Финальный грep-аудит (§3-подтверждение): `try_parse|parse_f64|__parse` в emit_c.rs —
  ТОЛЬКО остатки живой (не тронутой) `try_parse`-ветки для int/u64/u32/u16/u8/i32/i16/i8/f32/
  bool/char (вне скоупа) + имя C-хелпера `nova_parse_f64_result`/`nova_str_to_f64` (conv.h,
  существовал ДО фикса, используется try_from/try_parse-f32-веткой); НИ ОДНОГО упоминания
  `__parse_f64_opt` или иного parse-family имени, специфичного для НОВОГО `f64.parse`, не
  осталось нигде в компиляторе — resolution идёт целиком через обычный
  declared-function/FFI путь.
  ТУДА ЖЕ: `char.try_from(cp int) -> Result[char, CharTryFromError]` (единственный статик char
  без infallible-сиблинга, R3-нарушение) → `char.from(cp int) -> Result[char, CharFromError]`
  (+ренейм типа ошибки). Source of truth — `compiler-codegen/src/codegen/runtime_registry.rs`
  (std/runtime/char.nv авто-генерируется оттуда, `nova regen-runtime`); ренейм ТАМ принят
  как есть (существующий registry — не новый компиляторный хардкод per correction п.3).
  Call-sites мигрировано ~25 (полный греп, шире оценки ~6-8): std/runtime/{string/core.nv×2,
  defaults.nv×2, read_buffer.nv}, std/_experimental/{crypto/bcrypt.nv×2, encoding/hex.nv×2,
  encoding/url.nv×3, identifiers/ulid.nv×2, identifiers/uuid.nv×2}, std/encoding/{json.nv×2,
  base64.nv×3, utf16.nv}, std/http/url.nv, std/unicode/cp_utils.nv, std/testing/property.nv,
  std/prelude/{errors.nv (декларация), collections.nv (import+комменты)} + комменты в
  nova_tests/{runtime/from_into_basic.nv (4 INT-arg теста переименованы), syntax/as_cast_*,
  plan91_13/from_codepoint_test.nv} + spec_tests/conformance/partial_prelude/
  d371_partial_prelude_collections.nv (комменты). НЕ тронуты (другой, str-аргумент,
  compiler-hardcoded `T.try_from(str)`-путь, отдельный от char.from(int)): from_into_basic.nv
  str-тесты (109/116/122), str/conversions_err.nv (все — str-arg builtin, скоуп не этот).
  Гейты: сборка nova-cli+compiler-codegen (nova-lsp тоже, unaffected) OK; conformance 54/0
  (delta 0); std/encoding/json_test 24/24; std/encoding/serde/json PASS; std/_experimental/
  math/complex CC-FAIL — **pre-existing**, см. `[M-static-selfreturn-value-mangle-conflict]`
  ниже для root-cause и репродукции (НЕ про parse_f64_or_err — сгенерированный C для
  мигрированной функции корректен, подтверждено грепом `nova_fn_...parse_f64_or_err`/
  `nova_str_parse_f64` в .c); std/runtime/
  read_buffer standalone CODEGEN-FAIL / nova_tests/runtime folder-CU CODEGEN-FAIL (str.len()
  retired, gc_introspect.nv/memory_growth.nv, датировано 2026-06-17) — **pre-existing,
  изоляция вне обычного CU-контекста** (в реальном потребителе — nova_tests/buffers/
  read_char_str — PASS); nova_tests/plan91_fe2/neg/parse_int_overflow_err RUN-FAIL —
  **pre-existing** (parse_int body byte-identical, не тронут этим заходом).

- **[M-serde-slice-generic-method-parse]** ✅ **FIXED** (2026-07-07, parser/mod.rs) —
  корень: `parse_fn`'s slice-receiver arm (`[]T`, D38) parses the element type via the
  general `parse_type()`, whose `Ident` arm greedily continues a DOTTED qualified-path
  (`T.deserialize`) and then reads the trailing `[...]` as generic type-ARGS (no bounds) —
  so `fn[T Deserialize] []T.deserialize[D Deserializer](mut d D) -> ...`
  (serde.nv:311, static D42 receiver) folded `T.deserialize` into one path, then choked
  parsing the method's OWN generic-decl `[D Deserializer]` as instantiation-args, failing
  at 311:37 (`expected ], got identifier` on the bound name). The instance form
  (`[]T @serialize[S ...]`, serde.nv:303) never hit this — `@` doesn't continue a dotted
  path, only `.` does. Fix: new single-consumption context flag `receiver_elem_ctx`
  (mirrors the existing `pointee_ctx` pattern) set by `parse_fn` right before parsing the
  slice-receiver element type; `parse_type`'s `Ident` arm skips its dotted-path
  continuation loop when the flag is set, and the Array/FixedArray arms re-arm it before
  their own recursive `parse_type()` call so it survives arbitrary slice depth
  (`[][]T.method`). Verified: minimal 1-line repro (`fn[T Deserialize] []T.deserialize[D
  Deserializer](mut d D) -> Result[[]T, int] { Ok([]) }`) now parses (progresses to a
  semantic E_BOUND_UNKNOWN on the fabricated bound, not a syntax error); real
  `std/encoding/serde/serde.nv` (folder-CU) now compiles and its `json` sub-test runs
  (PASS 1/1); `nova_tests/serde` and `nova_tests/serde_e2e` and
  `nova_tests/http_typed/typed_json_test` now compile+run (previously CODEGEN-FAIL,
  whole-CU blocked) but surface a NEW downstream runtime gap — see
  `[M-slice-static-deserialize-garbage-len]` below. Gates: conformance 54/0 (delta 0 vs
  pre-fix), `std/encoding/json_test` 24/24 (unaffected, unrelated module).

- **[M-slice-static-deserialize-garbage-len]** ✅ **FIXED (2026-07-07, ветка cg-three-fix,
  emit_c.rs).** Два корня: (1) диспатч — array-ext static `[]T.method` эмитился ОДНИМ
  erased base с элементом, захардкоженным в `nova_int` (`Nova_NovaArray_nova_int_static_*`,
  тело строит `Nova_Vec____nova_int`); каждый call-site `Vec[<elem>].method` переиспользовал
  этот единственный nova_int-mono → str/record-элемент в nova_int-Vec = garbage `.len()` /
  CC-FAIL. Фикс: turbofish-static путь мономорфизирует per-element (bind receiver-typevar →
  `<elem>`, method-level generics из аргов, эмит `Nova_NovaArray_<elem>_static_<method>`;
  int пропущен = erased base байт-идентичен). (2) return-type — return-inference turbofish-
  static вызова деградировал до РАЗВЁРНУТОГО типа, `?` падал в `/* ? */` no-op, присваивая
  сырой `NovaRes_*` в `Vec`-локаль (garbage `.len()`). Фикс: Try-codegen восстанавливает
  Result-тип из ДЕКЛАРИРОВАННОГО return метода (`mono_method_decls[("[]T", method)]`) с
  typevar→`<elem>` (обобщение прежнего Ident-only `.deserialize`-fallback на любой turbofish-
  ресивер). Оживило serde/serde_e2e/http_typed (все RUN-FAIL → PASS). Регресс-гард:
  `spec_tests/conformance/slice_static_generic_method.nv`. Прежний текст (диагностика ДО
  фикса) ↓. — NEWLY SURFACED
  (2026-07-07) by `[M-serde-slice-generic-method-parse]`: this exact static-receiver form
  (`[]T.deserialize[D ...]` / call-site `Vec[T].deserialize(...)`) never compiled before,
  so it was never runtime-exercised. Repro: `nova_tests/serde_e2e/roundtrip.nv` —
  `User.deserialize` (line 46) calls `ro tags = Vec[str].deserialize(s3)?` (line 54) for
  field `tags []str`; the returned `Vec` is corrupt — `tags.len()` immediately after the
  call (before any record construction) prints a garbage huge int (observed
  `2453244099680`, non-deterministic across runs) instead of `2`; JSON encode of the SAME
  value is correct (`"tags":["x","y"]` — so `@serialize`/instance-form round-trips fine,
  only the STATIC `.deserialize` form on a slice receiver is broken). Same symptom in
  `nova_tests/serde/autoderive.nv` (`#impl(Deserialize)` auto-derive of a `tags []str`
  field — `autoderive.nv:32,46`) and `nova_tests/http_typed/typed_json_test.nv` (3 asserts
  on `w.tags.len()`). Separately (may be same root or a sibling gap): a DIRECT top-level
  `json_encode[[]str](v)` / `json_decode[[]str](s)` call (bypassing any hand-written
  record wrapper) fails EARLIER, at bound-check: `[E_?] type Vec does not satisfy
  Serialize bound (in call to json_encode[T Serialize])` — the checker's generic-bound
  satisfaction for `[]T`/`Vec[T]`-aliased receivers doesn't find the `[]T @serialize`
  method (same dispatch-gap class as `[M-153.6-vec-hashmap-key-eq]`). Suspect for the
  garbage-len bug: call-site spelling `Vec[str].deserialize(...)` vs decl spelling
  `[]str.deserialize(...)` (D38 alias, `[]T` ≡ `Vec[T]`) diverging in receiver-name-based
  mangling/lookup for the STATIC dispatch path specifically (instance `@serialize` isn't
  affected — it dispatches on the runtime/structural receiver value, not on call-site
  spelling text). NOT fixed here — out of parser scope (checker/codegen, explicit
  boundary per Fix-1 instructions). Repro kept in `nova_tests/serde_e2e/roundtrip.nv` +
  `nova_tests/serde/autoderive.nv` + `nova_tests/http_typed/typed_json_test.nv` (all 3
  already RUN-FAIL in-tree; no new fixture needed).

- **[M-next-collect-value-record]** ✅ **FIXED (2026-07-07, ветка cg-three-fix, emit_c.rs +
  std/runtime/string/chars.nv).** `Next[T]`-blanket-терминатор (`fn[I Next[T]] I mut
  @collect() -> Vec[T]`, std.collections.vec_iter) не мономорфизировался для КОНКРЕТНОГО
  `value priv(type)`-итератора, чей элемент фиксирован аннотацией `#impl(Next[<elem>])`
  (CharsIter — `str.chars()`). Protocol-aware blanket-dispatch связывал внутренний typevar
  `T` (в bound `Next[T]` на receiver-typevar `I`) ТОЛЬКО через generic-instance inference
  возврата `@next()`, которая требует `____`-type-args на C-типе ресивера; у плоского
  value-record их нет → inference давал None, `T` оставался несвязан, тело `collect`'а
  `Vec[T].new()` эрейзилось в `Vec____Nova_T` (record schema missing → `codegen error:
  anonymous record literal: expected struct 'Vec____Nova_T_p'`). Из-за этого to_chars-
  миграция шла push-циклами. Фикс: при промахе generic-instance inference читать элемент
  прямо из `#impl(Next[<elem>])`-спеки ресивера (`type_impl_protocols`) — авторитетный
  конкретный binding. Плюс добавлен недостающий `#impl(Next[char])` на `CharsIter @next()`.
  Регресс: `nova_tests/protocols/iter/str_iters.nv` (`to_char_vec` → `s.chars().collect()`,
  снят push-loop обход). home **codegen — blanket-method inner-typevar binding**.

- **[M-d411-record-binding-destructuring]** ✅ FIXED (2026-07-07) — реализация D411:
  record-паттерн в биндингах ro/mut. Парсер: `{` после ro/mut уже парсился (парсер уже
  делегировал в общий `parse_pattern`, который уже знал record-паттерны из match — ноль
  новой грамматики). Irrefutability-проверка (Plan 53, `check_let_pattern_irrefutable`) и
  codegen-биндинг полей (`emit_record_destructure`, источник вычисляется один раз в tmp)
  уже существовали в конвейере до D411 — унаследовано через retraction `let`→`ro`/`mut`.
  Новое: код-тег `[E_REFUTABLE_BINDING]` на существующую refutability-диагностику;
  НОВАЯ проверка `..`-правила (частичный список полей без `..` → `[E_RECORD_PATTERN_NEEDS_REST]`,
  только для ro/mut-биндингов — `check_priv_pattern_recursive_inner` c флагом
  `enforce_binding_rest`, types/mod.rs). Архитектура десугара: pattern-native, БЕЗ
  отдельного AST-pass (см. D411 «Правило» в spec/decisions/03-syntax.md — обоснование).
  Тесты: `spec_tests/conformance/d411_record_binding_destructure.nv` (shorthand/rename/
  mut/вложенные/однократность вычисления источника через mut-counter side-effect) +
  `spec_tests/conformance/neg/d411_sum_variant_refutable_neg.nv` +
  `neg/d411_partial_no_rest_neg.nv`. conformance 54/0 → 56/0 (+2 neg, 0 регрессий).
  Потребители: json-лексер `Lexer @next_token`/`@read_number` (std/encoding/json.nv) —
  `ro {line, col, ..} = @` / `ro {pos: start, line: start_line, col: start_col, ..} = @`;
  json_test 24/24 без изменений. Known gap: record-паттерн, вложенный внутри tuple-элемента
  биндинга, не проходит `..`-правило (нет type-resolution на этом пути) — редкий кейс,
  задокументирован в спеке, не блокирует закрытие.

- **[M-unwrap-twins-retraction]** ✅ FIXED (2026-07-07, [sonnet]) — ретракция
  метод-близнецов операторов (амендменты D85/D86 в спеке): снесены из prelude/core.nv
  `Option/Result @unwrap()`, `@unwrap_or`, `@unwrap_or_else`; мигрировано `.unwrap()`
  ×80 → `x!!`, `.unwrap_or(v)` ×228 → `x ?? v` по всему дереву (std/ spec_tests/
  nova_tests/ examples/) — фактические числа выше плановой оценки (33/29), т.к. план
  считал по узкой выборке. Прецедентность: реальная грамматика (`compiler-codegen/src/
  parser/mod.rs parse_postfix`) показала `??`/`!!` в ОДНОМ постфикс-цикле с `.`/`()`/`as`;
  RHS `??` парсится `parse_unary()`→`parse_postfix()` (не полный `parse_expr`) — скобки
  нужны только если fallback сам бинарное выражение вне call/cast/литерала. Проверка
  всех ~230 реальных fallback-аргументов: везде постфикс-safe атомы — скобки НЕ
  понадобились ни разу (не подтвердилось предположение про цепочки). `unwrap_or_else`
  — 2 живых случая с доступом к error-значению переписаны explicit `match`
  (nova_tests/plan99/result_unwrap_or_else_migrated.nv), остальные — `??`.
  ТУДА ЖЕ: `@capacity()` РЕТРАКТИРОВАН у StringBuilder/HashMap/WriteBuffer (дубль
  канонического `cap()`, D9) — вызовы .capacity() → .cap() (HashMap/StringBuilder/
  WriteBuffer only; Vec-плана-60-эры/Channel/user-типы с собственным `@capacity()`
  — ВНЕ периметра, найдены и намеренно НЕ тронуты). 03-syntax.md: D117 "Что"/"Правило"/
  таблица длин (:1923-зона)/"Что отвергнуто"/forbidden-abbreviations приведены к
  cap()-канону (`cap` добавлен в mainstream-исключения, 3→4).
  И ТУДА ЖЕ: StringBuilder `@len()` → `@byte_len()` (D249, откат прежнего «удалён»);
  `@char_len()` ретрактирован (0 usages в std/) — codepoint-счёт теперь
  `.clone().into_str().chars().count()` (линза+терминатор через non-consuming clone).
  WriteBuffer `@len()` НЕ тронут (граница — байтовый буфер).
  json parse_hex/`code`: остались `int` — `str.from_codepoint(cp int)` (std/runtime/
  char.nv:16) принимает `int`, менять на `u32` создало бы лишний cast без пользы (см.
  инструкцию — «если int, оставь int»).
  Найден и НЕ исправлен (вне периметра — компилятор): **[M-cap-getter-fluent-alias-
  false-positive]** — см. отдельный пункт ниже.
  Гейты: conformance 56/0 (дельта 0, эталон обновлён D411-мержем в процессе), std/
  encoding/json_test 24/24, HashMap/WriteBuffer/Vec targeted PASS, StringBuilder
  targeted PASS в изоляции (полный `nova_tests/strings/` как один CU уже упирается в
  ДВА пред-существующих несвязанных дефекта — см. отчёт агента).

- **[M-cap-getter-fluent-alias-false-positive]** (2026-07-07, P3, найден при
  [M-unwrap-twins-retraction], compiler defect, НЕ ФИКШУ — вне периметра задачи) —
  checker's `recv_returning` registry (`types/mod.rs` ~20392/20458) ключуется ТОЛЬКО
  по `(receiver_type, method_name)`, БЕЗ arity. StringBuilder одновременно объявляет
  0-arg getter `@cap() -> int` и 1-arg fluent setter `mut @cap(n) -> @` под тем же
  именем; т.к. StringBuilder — `consume`-тип, D180 Rule 2 alias-эвристика
  («`let x = recv.fluent_method()` ⇒ x aliases recv») в `check_stmt`/`Stmt::Let`
  срабатывает на 0-arg getter тоже → `ro x = sb.cap()` ложно даёт
  `[E_VIEW_BINDING_FORBIDDEN]`. Repro: `nova_tests/strings/str_builder_metrics.nv`
  (2 живых сайта, workaround `+ 0` на месте с маркер-комментарием — ломает точный
  AST-паттерн `Call{Member{Ident}}` без изменения значения). Фикс — arity-aware
  `recv_returning` (ключ должен включать arity/сигнатуру, не только имя).

- **[M-sync-test-stale-duplicate]** (2026-07-07, P2, Wave: санация-182 Ф.2) — std/runtime/
  sync_test.nv объявляет `module runtime.sync` (не sync_test) и ПОВТОРНО объявляет
  MemOrdering из sync.nv — устаревший файл-дубликат, не мигрированный на *_test-конвенцию;
  валит юниты sync и sync_test в --full std/runtime (бисект 2026-07-07: pre-existing на
  всех точках). Снести/переписать по test-conventions.
- **[M-runner-testless-units-main-impl]** (2026-07-07, P2, Wave: заход test_runner вместе с
  [M-test-runner-shared-temp-collision]) — **✅ FIXED** (2026-07-07, worktree `nova-runner`/
  `runner-fixes`). 8 юнитов std/runtime (char/defaults/fibers/gc/math/numeric/raw_mem/runtime)
  падали линковкой `nova_fn_main_impl`: раннер собирает exe для модулей БЕЗ test-блоков.
  Фикс: `codegen_to_c` (test_runner.rs) теперь после codegen (на ФИНАЛЬНОМ смёрженном `module`,
  учитывает folder-module peer-merge) считает `has_runnable_entry` = есть ≥1 `test "..."` блок
  ИЛИ явный top-level `fn main()` (bench-блоки не считаются — `nova test` не включает bench_mode,
  emit_main_wrapper их ветку не берёт). `run_one` при `!has_runnable_entry` возвращает новый
  `SkipReason::NoEntryPoint` СРАЗУ после того как .c записан (компиляция уже проверена) — БЕЗ tmp
  subdir/cc/link/run (самая дешёвая точка обрыва). Codegen-ошибки при этом НЕ маскируются: путь
  «SKIP» достижим только если codegen реально успешен. Приёмка `nova test --full ../std/runtime`:
  было 0/13 → стало PASS 0 / FAIL 4 / SKIP 9 (8 из маркера + `write_buffer.nv`, тоже безтестовый —
  раньше не диагностирован по имени, теперь корректно SKIP той же логикой); остаток 4 FAIL —
  pre-existing вне этой зоны (2× partial-prelude: read_buffer/`char.from`,
  string_builder/`str.from_bytes_unchecked_steal`; 2× sync-дубль: sync/sync_test, см.
  [M-sync-test-stale-duplicate]). Conformance `spec_tests/conformance --full`: 56/0, дельта 0
  против main.
- **[M-compiler-nv-porting-wave]** (2026-07-07, P2, Wave: [haiku+sonnet] сразу после вливания
  parse-family-fix — общие ветки try_from/try_parse) — по карте аудита §3 (отчёт агента
  2026-07-07): (D) снос ~383 строк мёртвых *_unused()-реестров runtime_registry.rs:493-887;
  (A) перенос nova_body-строк char/u8-конверсий в std/runtime/char.nv (:437-464);
  (B1) namespace-дедуп gc/bench/fibers/runtime — generic-lookup в external_registry.rs,
  снос 4 матч-блоков emit_c.rs:28448-28584 (.nv-декларации уже source of truth, gc.nv:47
  прямо просит); (B2) bit-cast перехваты from_bits/to_bits — снос 6 копий, реестр уже
  корректен; (B3) str.from(bool|char|f64|f32|int) — .nv extern-оверлоады + снос ветки
  (сверить дубль Nova_str_static_from_char vs nova_char_to_str — один путь мёртв);
  (B4) скалярные T.try_from(str) ×12 → СЛИТЬ с программой T.parse (174.1, R3-имя) —
  отдельный заход по канону parse.nv. Проверка: греп-инвариант новых имён в emit_c = 0.
- **[M-emit-c-dispatcher-triplication]** (2026-07-07, P2, Wave: 172.12-семья/A5) —
  ✅ ЗАКРЫТ 2026-07-08 (172.12 §14.19, коммиты `503394757`+`eab52620d`): обе infer-копии
  (channel-6q + недостижимый legacy-match Call-арм; 2485 строк дословно, 0 различий)
  схлопнуты в единый хелпер `infer_call_ret_c`; −2476 строк. «Третья копия» ~28300 =
  `emit_call` — не копия инференса (эмиссия, `&mut self`, `Result`), остаётся своим
  каналом по построению. Фикс интринсика отныне = 1 infer-правка (+1 emit при
  необходимости), было 2-3 синхронных.
- **[M-checker-builtin-mut-method-list]** (2026-07-07, P3, Wave: 185/172-семья) —
  is_builtin_mut_method (types/mod.rs:23146) — checker-эвристика со списком mut-методов
  Vec/Map/Set, дублирует знание о реальных .nv-методах (дрейф-риск класса gc.nv).

- **[M-plan62-hashable-flap-runtime]** (2026-07-07, P2, Wave: разбор 17 крашеров санации-182 —
  тот же класс рантайм-нестабильности) — nova_tests/plan62/duplicate_hashable_protocol.nv
  флапает PASS/RUN-FAIL в single-process ПОСЛЕДОВАТЕЛЬНЫХ прогонах (обрыв на 4-й из 90
  проверок) — НЕ temp-гонка (доказано runner-фиксом: PID-изоляция не убрала флап). Похоже
  на GC/рантайм-гонку в самом тесте или кодогене hashable-протокола.

- **[M-bang-requires-fail-enforcement]** (2026-07-07, P2, Wave: §4а-остаток, чекер-заход) —
  по D85 `!!` = throw через Fail и вне Fail-контекста должен отвергаться чекером
  (E_BANG_REQUIRES_FAIL), фактически компилируется (найдено владельцем на
  json.nv read_hex_quad). Заход: (1) выяснить текущий лоуэринг `!!` без Fail (тихий
  unchecked? panic?) — если тихий unwrap, это P1-звучность; (2) enforcement + канон-миграция
  не-Fail мест на `?? panic("...")`/честный Fail (вкл. сеттеры @header/mock `insert(...)!!`
  из http-волны); (3) проверить, несут ли test-блоки неявный Fail-контекст (масса `!!` в
  тестах — легальность зависит от этого).

- **[M-porting-wave-tails]** ✅ FIXED (2026-07-07, P3, ветка porting-tails @071411a38; п.2 не чинится, адрес 174.1) — три
  хвоста porting-волны §3: (1) диагностические подсказки emit_c.rs (~:39828) советуют
  снесённый `char.try_from(n)?` — обновить на `char.from(n)?` ✓; (2) `f64.try_from(str)`
  возвращает некорректное значение (задокументировано в from_into_basic.nv — якорь
  [M-f64-try-parse-to-parse-f64]: known-broken, ретракция программе T.parse 174.1); (3) `emit-runtime-stubs`
  авто-генерирует пустой stray std/runtime/string.nv — str_runtime() → vec![] + check в main.rs ✓.

- **[M-ptr-raw-access-contract-and-unaligned]** ✅ РЕАЛИЗОВАНО (2026-07-08, sonnet,
  ветка `ptr-174-5` @a23cb794b, база b16ee25e0; приёмка владельца — отдельно) (2026-07-07, P2,
  Wave: план 174.5 — методы указателей; ПЕРЕРЕШЕНО владельцем: `.read()`/`.write()` НЕ получают
  memcpy-семантику) —
  (1) D141-амендмент: read/write на сырых указателях = голый deref (как сейчас,
  emit_c.rs:29141) с ЯВНЫМ контрактом «требуется выравнивание и same-type aliasing, иначе
  UB» (Rust-канон ptr::read); (2) добавить ОТДЕЛЬНЫЕ `@read_unaligned()`/`@write_unaligned()`
  с memcpy-эмиссией (typed inline-хелперы, канон Plan 145) — для сетевых парсеров/174.5;
  (3) from_bits/to_bits ПЕРЕНОСЯТСЯ в чистый .nv ПОВЕРХ read_unaligned (финальные формы
  владельца: `fn f64 @to_bits() -> u64 => unsafe { (&@ as *u64).read_unaligned() }` и
  `fn f64.from_bits(bits u64) -> f64 { unsafe { (&bits as *f64).read_unaligned() } }` —
  адрес прямо от ro-параметра, без mut-локала) — extern-записи, реестр и C-обёртки numeric.h
  сносятся; имена подтверждены по D410-осям (to_bits = новое владеющее значение, не вид);
  (3а) закрепить правило: `&` на ro-биндинге/параметре легален → ro-указатель `*T`,
  на mut → `*mut T` (Rust-параллель &/const); если чекер сегодня требует mut — снять;
  unsafe-границу каста/чтения сверить по D54/L3; (4) size_of-API — отдельный пункт 174.5 (const-фича).

  **Сделано (2026-07-08, ветка `ptr-174-5`):** (1) явный D141-контракт закреплён комментарием
  на месте (голый deref остаётся, как решено); (2) `.read_unaligned()`/`.write_unaligned(v)`
  добавлены (memcpy через compound-literal + `memcpy`-return-dest трюк, MSVC-portable, без
  stmt-expr) во всех трёх живых копиях дispatcher-блока emit_c.rs; (3) `numeric.nv` переписан
  дословно по форме владельца (extern/registry/numeric.h снесены; `#no_prelude`
  write_buffer.nv/read_buffer.nv + prelude/collections.nv получили explicit
  `import std.runtime.numeric`, т.к. to_bits/from_bits перестали быть always-available
  compiler intrinsic); (3а) `&ro as *T`/`&mut as *mut T` уже были легальны (mut чекером не
  требовался) — НО дословная форма владельца без внешних скобок (`&expr as *T`) не парсилась
  (as поглощал operand AddrOf раньше знака `&`) — узкий parser-фикс (re-ассоциация в Amp/
  RawAddrOf ветках parse_unary), 0 regressions (грепом подтверждено 0 существующих
  использований старой ассоциации); (4) `size_of[ref T]()` уже отвергается E_REF_TYPE_POSITION
  (общий walk_typeref-путь) — только verify, добавлен neg-тест. Conformance 66/0 → 67/0 (+1
  neg-файл; 2 pos-файла вошли в общий CU без изменения счётчика). Targeted-фикстуры:
  `spec_tests/conformance/d141_ptr_bitcast_roundtrip.nv`,
  `spec_tests/conformance/d141_ptr_read_write_unaligned.nv`,
  `spec_tests/conformance/neg/d326_size_of_ref_typearg_neg.nv`.

  ТУДА ЖЕ (вопрос владельца про ADDR_IMAGE_BYTES, 2026-07-08): при появлении size_of-API
  описать net-образ типизированной записью и заменить литерал 20 на size_of[...] —
  единственный источник истины (как Rust libc::size_of / Go cgo-godefs+Sizeof);
  C-сторона уже защищена _Static_assert(sizeof(NovaNetAddr)==20).
- **[M-http-module-test-block-p67]** (2026-07-07, P1-разведка, Wave: заход крашеров-182
  [opus] — то же P67-семейство) — ЛЮБОЙ test{}-блок в `module std.http` (даже no-op) валит
  компилятор: `[P67-LEGACY] Ident 'msg' not in var_types` (санация-182, три независимые
  репродукции). Блокирует возврат http-тестов body/model/url/d358 из nova_tests к модулю
  (якоря в файлах). Родня [M-172.1-var-types-cu-name-leak]/ErrorKind-коллизии.

- **[M-per-file-check-no-prelude-protocol-scope]** (2026-07-08, P2-разведка, класс per-file
  check — conformance НЕ видит) — per-file `nova check` файла из `#no_prelude`-CU
  `std.runtime.string` (chars.nv / core.nv) падает: E_BOUND_UNKNOWN `Iter`/`AsSlice`
  (vec/mutate.nv:239/260 — bounds резолвятся только при prelude в walk),
  E_IMPL_UNKNOWN_PROTOCOL `Next`/`AsSlice` (vec/iter.nv:38, vec/access.nv:267,
  string/chars.nv:76), E_READONLY_CONTENT (string/core.nv:159 — `ro buf = RawMem.alloc(...)`,
  `buf[n]=0`: тип `*mut u8` теряется в этом же режиме). Транзитивно подтянутый
  std.collections.vec ссылается на протоколы из std/prelude/{collections,protocols}.nv,
  которые per-file walk #no_prelude-CU не включает. НЕ регрессия 174.5 — цепочка бисекции
  (2026-07-08, приёмка 174.5): (1) бинарь@b16ee25e0 + дерево@b16ee25e0 → FAIL;
  (2) тот же бинарь + pre-merge main@1d75fcfef → FAIL (между base и merge только
  .nv-стиль-коммиты, компилятор не менялся); (3) бинарь ветки ptr-174-5 + её дерево → FAIL
  с идентичным списком. Полный whole-CU режим (nova test/conformance) — зелёный.
  Родня per-file/CU-scope семейства [M-http-module-test-block-p67].

- **[M-fixed-array-value-semantics]** (2026-07-08, P2) — ✅ **ЗАКРЫТ (2026-07-10, ветка
  `fixed-array-value` [sonnet], коммиты ab322cede/b4b699276/4043a4a51/ca39d9d06 + финальный):**
  [N]T = inline value-класс. (1) `ResolvedType::FixedArray(N, elem)` — N без потерь (закрыл
  [M-172.1-fixedarray-N] для C-лоуэринга; category-key `resolved_cat_of` НЕ тронут);
  V3-классификатор + ref_target_confirmed_heap переклассифицированы (WIP 5ff32af0d).
  (2) codegen: `typedef struct { T data[N]; } _NovaFixArr_<N>_<L>_<T>` (finalize-splice,
  топосорт [N][M]T), compound-literal для литералов, Index-чтение/Assign-запись с
  bounds-check по компайл-тайм N (`nova_fixarr_idx_chk/nochk`, array.h), return-коэрция,
  `is_value_type()` → in-out ABI D326 Р10 для `mut x [N]T`. (3) ДЕФЕКТ ВОЛНЫ (пойман
  sha256 NIST): field_cache писал `@F[i]=v` в кэш-копию — фикс: index-write барьер для
  slot-unstable полей (ref-typed []T/Vec байт-в-байт нетронуты). (4) gc_layout: [N]T-поле
  = N×stride inline офсетов (юнит 22/22); GC-тест [4]str/[3]Holder под 3× gc.collect() —
  пейлоады удерживаются (стек + heap-record скан). (5) Спека: D27-амендмент (5 пунктов) +
  D216 §V3.1 value-список п.6 + Rust-снипет. Тесты nova_tests/fixed_array/ 5 pos + 2 neg +
  panics (D348). Гейты: build оба чисто; conformance 89/0 δ0; err173 25/0 δ0;
  выборка nova_tests (11 каталогов) δ0 против baseline-бинаря main@250de5cda
  (FAIL-множества идентичны, все pre-existing); sha256/sha1/md5/hmac PASS.
  **Followups:** (а) serde-derive видит [N]T как Vec (auto_derive.rs deser) — при
  [N]T-поле в serde-типе будет клэш типов, живых пользователей нет;
  (б) len-mismatch/spread-в-литерале ловятся codegen loud-fail — чекер-уровневый
  E-код чище; (в) external_registry.rs FixedArray-арм лоуэрит в C-тип ЭЛЕМЕНТА —
  мёртвый код (extern fn с [N]T в сигнатурах нет; D326 Р9: FFI = только сырые указатели).
- **РЕШЕНИЕ ВЛАДЕЛЬЦА (2026-07-08) по [M-array-vec-unify]: Vec-canon — NovaArray умирает
  целиком** (вариант typedef-alias отвергнут). Substrate-очерёдность (вердикт A5):
  A6 = runtime-примирение (nova_str_to_chars/bytes/split и потребители read_buffer/
  string_builder/cast.h → Vec-образы), A7 = координированный codegen-флип 5 сайтов
  (binding-type, receiver_c_type, .new(), литералы ×3, for-loop) + снос NOVA_ARRAY_DECL-
  набора array.h (uint-маркер закрывается автоматически), затем коллапс триплификации.
- **A6 ЗАКРЫТ (2026-07-07, заход 10, 172.12-typed-ir-mono §14.7-14.11):** str-семейство
  runtime-примирение выполнено. ЭМПИРИКА опровергла предпосылку A5 §14.4.1 «NovaArray
  runtime-встроен, НЕ выпиливаем»: str-C-функции (`nova_str_to_bytes`/`bytes`/`as_bytes`/
  `split`/`to_chars`/`chars` в array.h + `from_bytes_unchecked`/`steal_bytes`/`from_bytes_lossy`
  в string_builder.h) **МЁРТВЫ** — ноль call-site в генерируемом C, ноль FFI/extern (все
  ссылки = doc-комментарии). Retired роутингом Plan 139.2 → Nova-body методы строят
  реальный `Vec[u8]`/`Vec[str]` через `Vec[T].from_raw_parts`; even `ro b = s.bytes()`
  даёт `Nova_Vec____nova_byte* b = Nova_str_method_bytes(s)` (checker-канал перекрывает
  legacy infer). cast.h — БЕЗ NovaArray-ссылок (уже чист). **Сделано:** (1) снос 9 мёртвых
  C-функций (byte-identical: `nova_rt/*.h` = `#include`, не splice в .c; `static inline`+unused
  → 0 object-code); (2) align 8 устаревших infer-fallback'ов NovaArray→Vec (обе триплет-копии:
  str.bytes/split, WriteBuffer.into, ReadBuffer.remaining_bytes — все Nova-body `[]u8`/`[]str`,
  перекрыты каналом). **Typedef-слой-вопрос (NovaVecImage vs real-mono-forward-decl) МООТ** —
  ни одна rt-функция не называет Vec-mono (str уже на Nova-body), 0 ре-деклараций/клэшей по
  построению. Гейты: build оба крейта clean, conf **66/0** δ0, byte-identity 36/40 identical
  (4 diff = generic-stub fwd-typedef ordering-нондетерм., base-vs-base доказано; **NovaArray→Vec
  дельта-список ПУСТ** — канал перекрывает fallback → 0 emitted-C дельты), nova test выборка
  (strings/str/buffers/unicode/json/serde/compress/plan108/145/runtime) δ0 (все fail = pre-existing
  stale-тесты retired API `CharsIter.nth`/`str.len()` D249, идентичны на baseline). **A7 de-risked:**
  str-runtime уже Vec → A7 = только user-`[]T` codegen-флип 5 сайтов (`receiver_c_type:13901` —
  единственный оставшийся широкий NovaArray-эмиттер, `.new()`, литералы ×3, for-loop, type-map
  `:2978-2992`/`:14127`) + `NOVA_ARRAY_DECL`-снос (uint-маркер закрывается там).

- **[M-ffi-prefix-reversal]** (2026-07-08, P2, Wave: [haiku] СРАЗУ после вливания
  no-underscore-prefix — общая зона std/net) — ПЕРЕРЕШЕНИЕ владельца по FFI-канону:
  extern "nova" манглится компилятором в nova_fn_* (неймспейс уже есть); extern "C" — имя
  как есть, vendor-префикс без причины не нужен. Канон: модульные C-шимы = `<модуль>_*`
  БЕЗ nova_ (net_addr_loopback_into, os_env_get, str_parse_f64). ЯДРО rt С nova_ остаётся
  (ABI языка: nova_int/nova_str типы, nova_alloc, паника, манглы nova_fn_/Nova_*).
  Миграция-разворот: откат вчерашней канонизации nova_fs_/nova_os_/nova_io_ → fs_/os_/io_
  (~50, были правильными); nova_net_* ×44 → net_*; nova_brotli_* ×7 → brotli_*;
  conv.h-семейство nova_str_to_*/nova_str_parse_f64 → str_*; синхронно .nv-декларации +
  rt-хедеры + внутренние вызовы rt. Правило — строкой в compiler-conventions §FFI
  (или nv-coding-style) с датой.

- **A7 старт ЗАКРЫТ (2026-07-07, заход 11, 172.12-typed-ir-mono §14.12-14.15):** координированный флип
  `.new()/.with_capacity()` + `receiver_c_type`. Эмпирика: 3 из 5 «широких сайтов» из инвентаря заход-10
  (binding-type, array-литералы, for-loop) оказались УЖЕ Vec-native ДО этого захода (`resolved_array_to_c`
  Vec-flip + `try_emit_typed_vec_literal` + iter()/next()-протокол уже покрывали primitive `[]T`) — реальный
  разрыв был только в static-ctor dispatch (scalar `[]T.new()` эмитил legacy `nova_array_new_<suffix>` в
  Vec-типизированный биндинг — layout-совместимо, текстуально рассинхронизировано) и в `receiver_c_type`'s
  редком extension-method-receiver арме. Оба закрыты: `elem_needs_vec_mono` → `elem_is_erased` (legacy остаётся
  ТОЛЬКО для генуинно нерезолвленного type-param — erasure-sentinel, ортогонально), `receiver_c_type`'s `[]T`
  манглит+регистрирует `Vec[elem]` (uint → `nova_uint`, ЗАКРЫВАЕТ `[M-uint-legacy-array-uint64-until-a4]`
  для этих путей). Гейты: build clean, conf 66/0 δ0 (d27/d38/d403/d232 PASS), uint round-trip PASS
  (`Nova_Vec____nova_uint`), wide nova test 18/2/1skip (2 fail байт-в-байт идентичны baseline, pre-existing
  D249), спот-C-check 0 `NovaArray_` для []int/[]uint user-кода. **НЕ сделано** (контингенси плана):
  `NOVA_ARRAY_DECL`/`IMPL`-снос — найдены 3 доп. блокера ВНЕ инвентаря заход-10 (Plan 96 parallel-for
  `emit_c.rs`~9990-10090, array rest-bind destructuring ~37100-37117, `resolved_array_to_c`'s dead-in-practice
  primitive-табличка ~2977-2993) — все строят/упоминают сырые `NovaArray_<T>*` напрямую, нужен тот же
  мангл+регистрация паттерн до сноса макросов. `emit_array_lit`'s closure/void_p/protocol-box fallback
  НАМЕРЕННО остаётся NovaArray (closure-arrays вне области A7, `NOVA_ARRAY_DECL(void_p)` должен остаться).
  Следующий заход: закрыть 3 блокера → снос array.h (кроме void_p).

- **A8 ЗАКРЫТ (2026-07-08, заход 12, 172.12-typed-ir-mono §14.16-14.18):** 3 блокера закрыты +
  снос `NOVA_ARRAY_DECL/IMPL`-набора array.h ВЫПОЛНЕН. (1) Plan 96 parallel-for (4 суб-сайта) и
  (2) rest-bind — на Vec[elem]-мангл через новый общий хелпер `vec_mono_ctor_push`; (3)
  `resolved_array_to_c`'s primitive-табличка: «dead-in-practice» из A7-журнала ОПРОВЕРГНУТО
  (достижима через D62.F partial-prelude без `collections`; сценарий не покрыт тестами и уже
  нефункционален по несвязанным партиал-прелюд пробелам) — Vec-гейт снят, оба пути слиты в одну
  Vec-конструкцию; соседний `emit_array_lit` legacy-fallback (non-void_p) → loud-fail с
  диагностикой. Снос array.h: 11 примитивов + nova_str-блок + compare_nova_byte → `NOVA_OPT_DECL`
  (Option-структуры остаются); ОСТАЛИСЬ nova_int (erasure-sentinel) + void_p (closure/protocol-box).
  Сопутствующая чистка stale-эмиттеров emit_c.rs (infer-твины []T.new(), ParallelFor-infer,
  apply_type_subst_to_ref primitive-арм → Vec-имена; placeholder-элементы НЕ Vec-манглятся —
  урок vec_seq). `[M-uint-legacy-array-uint64-until-a4]` закрыт ПОЛНОСТЬЮ. Гейты: build clean,
  conf 66/0 (d27/d38/d403/d232 явные PASS), пробы parallel-for/rest-bind PASS, широкая выборка
  δ0 (все FAIL pre-existing байт-в-байт: D249 strings, E7320/E_EXT_IMPORT collections, P67 json),
  спот-греп .c: 0 NovaArray_ вне sentinel/void_p. Попутные находки (§14.18): D71 `parallel for`
  без явной аннотации — pre-existing checker-пробел (`infer_expr_type` без ParallelFor-арма,
  binding=nova_unit; codegen корректен); partial-prelude+[]T теперь чистый loud-fail
  ([M-partial-prelude-primitive-method-registry]). Коммиты 0292f3694/65d165e75/99b3d2ce2.

- ✅ ЗАКРЫТ (Plan 186, 2026-07-09; статус-синк 2026-07-26) **[M-hex-blob-embed-d412]** (2026-07-07, P2, Plan: **186** (docs/plans/186-hex-blob-embed.md), очередь после
  172.12-A8/174.5; Wave: [sonnet] по карте плана 186) — реализовать D412: (1) лексер
  `x"…"`-литерала (hex-цифры + разделители `_`/пробел/перенос; E_HEX_BLOB_ODD,
  E_HEX_BLOB_CHAR) → компайл-тайм `[]u8`; (2) интринсик `embed("path")` (путь-литерал
  относительно .nv-исходника, E_EMBED_NOT_FOUND, файл в fingerprint сборки);
  (3) эмиссия `static const uint8_t nova_blob_<n>[]` + материализация: ro-биндинг =
  нулевая копия (data→статика, len==cap), mut-биндинг = копия в GC-кучу в точке
  биндинга; (4) тесты: pos (ro-вид, mut-копия, пустой x"", группировки, embed
  round-trip) + neg (нечёт, не-hex, отсутствующий файл); спека D412 в 03-syntax.md.

- ✅ **[M-fs-tls-mn-race]** (2026-07-08, **P1 — ЗАКРЫТО** (проверено 2026-07-11: fs-M:N-переписыванием, см. [M-sched-park-concurrent-fs]);
  латентная гонка данных, Plan: 176.1-кандидат
  или отдельная волна; Wave: [sonnet] НЕМЕДЛЕННО, зона nova_rt/fs.* + std/fs свободна) —
  fs спроектирован на TLS-протоколах из нескольких вызовов и НЕ безопасен для M:N:
  (1) stat-семейство: `fs_stat(path)` кэширует uv_stat_t в TLS → `fs_stat_size()/mtime/...`
  читают TLS без аргумента; (2) `fs_realpath()` → `fs_realpath_data()` — то же;
  (3) `fs_scandir()` → `next()/name()/kind()` — итератор в TLS. При преемпции (Plan 44.7
  sysmon) файбер мигрирует между тредами МЕЖДУ двумя соседними вызовами → чужой/пустой
  TLS; два файбера на одном треде интерливом затирают TLS друг друга. net.c документирует
  правильный канон («result never transits a __thread slot — it lands in the parked
  fiber's own request») — fs от него отступил. Починка по net-паттерну: stat →
  `fs_stat_into(path, *img)` + аксессоры от указателя (STAT_IMAGE_BYTES = fs_stat_image_bytes(),
  как net_addr_size); realpath → результат сразу в GC-строку без TLS; scandir →
  handle-based (`fs_scandir_open(path)->h`, `next(h)/name(h)/kind(h)/close(h)`, хендл
  внутри Nova-записи DirIter — файбер-владение). Аудит остальных: net — безопасен by
  design; brotli — handle-based; io/conv — stateless; os_env — argc/argv process-wide
  после set_args (иммутабельно, ок); effects.c TLS — ядро шедулера (сохраняется на
  переключении), вне класса.

- **[M-sched-park-concurrent-fs]** (2026-07-08, **ЗАКРЫТО** — тот же корень, что
  [M-fs-tls-mn-race], уже исправлен fs-M:N-переписыванием; расследование [opus]) —
  обнаружено fs-M:N-волной: 2+ конкурентных файбера с блокирующей fs-операцией под
  real_fs() валили рантайм (`nova: nova_sched_park: invalid scope/slot`, а также SIGSEGV
  и lost-wakeup-зависание). Разведка: планировщик (fibers.h/nova_sched.h/runtime.c/
  effects.c) БАЙТ-В-БАЙТ идентичен между базисом воспроизведения 200d5a79a и main —
  между ними менялся ТОЛЬКО fs.c/fs.h/std/fs/*.nv. КОРЕНЬ: старый fs.c держал РЕЗУЛЬТАТ
  (`_fs_stat_tls`, `_fs_realpath_tls`) и — для scandir — ВЕСЬ запрос вместе с его
  (scope,slot) в `__thread`-слотах. Под work-stealing файбер, запарковавшийся на воркере
  A, возобновляется на воркере B и читает thread-local воркера B — запрос ЧУЖОГО файбера;
  это (а) отдаёт чужой результат и (б) в scandir-пути подставляет чужой/протухший
  (scope,slot) в nova_sched_park → `slot >= scope->count` (scope уже сброшен count=0) →
  abort; при других раскладах — SIGSEGV (resume не того файбера, порча арены) либо
  lost-wakeup. Гипотеза волны про «cross-thread wake с THREADPOOL» неверна: libuv-fs
  completion (`_fs_cb`) всегда идёт на loop-треде того же воркера, НЕ с threadpool —
  это ровно net-паттерн; проблема была не в wake, а в TLS-переносе результата.
  Эмпирика (изолированный компилятор воркти, clang): СТАРЫЙ fs (staged из 200d5a79a) —
  8-файберный hammer при NOVA_MAXPROCS=2 даёт ~6/20 зависаний + эпизодический SIGSEGV;
  НОВЫЙ fs (main, net-канон: caller-owned stat-image, GC-строка realpath, handle-based
  scandir — ноль fs thread-local'ов) — 80/80 чисто на MAXPROCS 1/2/4/16. Планировщик НЕ
  дефектен; его abort — корректный guard, срабатывавший на порчу (scope,slot) со стороны
  fs. Фикса в планировщике НЕ требуется (маскирующая проверка была бы обходом §4а).
  Регресс-страж закоммичен: `std/fs/concurrent_stat_test.nv` (2 и 16 конкурентных
  real_fs-файберов; зелёный на новом fs, ловит любой возврат fs-состояния в TLS).

- **[M-test-runner-module-aggregation-segv]** (2026-07-08, P2, Plan: 182-хвост/172.13;
  Wave: при заходе в раннер) — довливной SEGV агрегации: одиночные test-build КАЖДОГО
  из 8 std/fs/*_test.nv зелёные, но агрегация всех файлов модуля в один exe (target
  d323_path_ops_test.nv) падает SEGV на определённой комбинации; воспроизводится на
  базисе 200d5a79a. Родственно: раннер видит только 4 файла в std/fs при папочном
  прогоне (поведение базиса); атрибуция строки падения первому файлу модуля
  (access.nv вместо views_test.nv, замечено на vec 2026-07-07); self-import гэп
  vec_lazy/vec_iter (E_EXTENSION_METHOD_NEEDS_IMPORT на самих себя, 2026-07-07).

- **[M-sha256-array-repeat-literal-parser]** ✅ **WORKAROUND APPLIED 2026-07-08** — конкретный
  repro давно известного «array literal parser»-gap (STATUS.md `_experimental/crypto`: «4 FAIL»,
  `[M-183-gc-vec-value-heap-tracing]` уже упоминал «литерал-повтор `[0; N]` — известный отдельный
  gap» без marker-id). `std/_experimental/crypto/sha256.nv` (было ×2: строки 151, 174) —
  `mut out [32]u8 = [0; 32]` / `mut w [64]u32 = [0; 64]` (Rust-style array-repeat) →
  `expected `,` or `]` in array literal, got int literal` — парсер Nova НЕ поддерживает
  `[value; count]`-синтаксис. **Обнаружен как блокер промоушена `crypto/bcrypt.nv`** (волна
  std/_experimental→std, 2026-07-08): `bcrypt.nv` вызывал `Sha256.new()/.update()/.finalize()` БЕЗ
  явного import (резолвилось молча на что-то другое — WriteBuffer, `no member 'update'/'finalize'
  in Nova_WriteBuffer` CC-FAIL); добавление явного `import std._experimental.crypto.sha256.{Sha256}`
  вскрыло истинную ошибку — `sha256.nv` не парсился вообще. **Обход применён (вариант b, парсер НЕ
  тронут):** оба литерала переписаны на явные списки нулей (32/64 штук). Цепная разблокировка:
  `sha256.nv` → парсится, но затем вскрыл ЕЩЁ 2 независимых pre-existing дефекта в своей
  транзитивной зависимости `encoding/hex.nv` (тоже пофикшены тем же заходом): (1) `encode_with`
  — `buf.into()` (несуществующий метод, D133-not-consumed) → `buf.into_str()`; `with_capacity`→
  `.new()`+`.cap()`; (2) `Hex.decode`/`digit_value` — retired `s.len()` → `s.byte_len()`, throw→
  Result (см. `[M-177-experimental-fallible-migration]`); test-блок `[][]u8`-nested-array паттерн
  (`NovaArray_nova_int_p` CC-FAIL, отдельный nested-generic-array gap, НЕ зафиксирован отдельным
  marker'ом — обойдён per-value assert без Vec-обёртки, не мейнлайн-баг). `sha256.nv`+`hex.nv`
  теперь `nova test --full` ПОЛНОСТЬЮ зелёные — но остаются в `_experimental` (вне периметра
  14-модульной волны, только починены как транзитивная зависимость bcrypt). `crypto/bcrypt.nv`
  САМ **успешно промоутирован** этой волной в `std/crypto/bcrypt.nv` (throw→Result + все найденные
  попутные дефекты см. ниже) — парсер-фикс НЕ отменяет актуальность родового `[value; count]`-gap
  для будущего кода (тело задачи (a) остаётся открытым: поддержать grammar в парсере).
  **Попутно найдены и пофикшены НЕСВЯЗАННЫЕ реальные баги в `bcrypt.nv` самом** (не про
  sha256/hex): (1) match на tuple-из-4×`Option[u32]` (`bcrypt_base64_decode`) эмитил `.f0->tag`
  pointer-деref для value-embedded `NovaOpt_uint32_t` → CC-FAIL; обход — последовательный `?`-unwrap
  вместо tuple-match (тот же класс, что и match на `Option[EnumType]` ниже); (2) `.chars()`/`.bytes()`
  вызванные НАПРЯМУЮ на module-level `const BCRYPT_ALPHABET` резолвились в ошибочный path-call
  `nova_fn_CONST_method()` вместо instance-метода — обход: локальная `ro`-копия константы перед
  вызовом метода; (3) локальная переменная с именем `long` (C reserved keyword) ломала codegen —
  переименована в `long_pw`; (4) **функциональный баг** (не codegen): `bcrypt_base64_decode`'s
  trailing-логика покрывала ТОЛЬКО «2 leftover chars → 1 byte» (salt, 22 chars), СИЛЕНТНО теряя
  3-й leftover char и 1 байт для «3 leftover chars → 2 bytes» (hash, 31 chars) — round-trip
  `Bcrypt.hash`+`Bcrypt.verify` ломался на hash-порции; добавлена недостающая ветка `remaining==3`.
  Все три файла (`sha256`/`hex`/`bcrypt`) теперь проходят `nova test --full` PASS.

- **[M-match-tuple-option-value-deref]** (2026-07-08, P2, Wave: 172.13 чекер-каналы) —
  `match` по позиционному tuple-литералу из N×`Option[T]` (T — value-тип: numeric/enum) эмитит
  `.fN->tag` (pointer-деref через `->`) для каждого элемента, но реальная C-репрезентация
  `Option[u32]`/`Option[<enum>]` в этом контексте — value-embedded struct (`NovaOpt_uint32_t`
  с полями `.tag`/`.value`, НЕ указатель) → `error: member reference type 'NovaOpt_uint32_t' is
  not a pointer; did you mean to use '.'?`. Обнаружен в `crypto/bcrypt.nv`
  (`bcrypt_base64_decode`, `match (c0, c1, c2, c3) { (Some(a), Some(b), Some(c), Some(d)) => ... }`
  над `Option[u32]`×4) при промоушене волны std/_experimental→std 2026-07-08. Минимальная
  репродукция:
  ```nova
  fn decode4(a Option[u32], b Option[u32], c Option[u32], d Option[u32]) -> Option[u32] {
      match (a, b, c, d) {
          (Some(w), Some(x), Some(y), Some(z)) => Some(w + x + y + z)
          _ => None
      }
  }
  // вызов из test-блока → CC-FAIL:
  //   member reference type 'NovaOpt_uint32_t' is not a pointer; did you mean to use '.'?
  ```
  **Обход в `bcrypt.nv`:** заменён на последовательный `?`-unwrap каждого `Option` вместо
  tuple-match (семантика идентична: любой `None` → ранний `return None`), см. комментарий в коде
  у `bcrypt_base64_decode`. Тело задачи: match-lowering для tuple-of-Option (или generic
  tuple-of-value-type) должен использовать `.` для value-embedded вариантов Option, симметрично
  тому, как уже резолвится одиночный (не-tuple) `match opt { Some(x) => ... }`.

- **[M-module-const-chars-bytes-resolution]** (2026-07-08, P2, Wave: 172.13) — вызов instance-
  метода (`.chars()`/`.bytes()`, возможно шире) НАПРЯМУЮ на module-level `const str`-значении
  (не на локальной переменной) резолвится в ошибочный path-call `nova_fn_<CONST_NAME>_<method>()`
  вместо инстанс-метода на строковом значении — типы не совпадают, CC-FAIL. Обнаружен в
  `crypto/bcrypt.nv` (`BCRYPT_ALPHABET.chars()` внутри `bcrypt_decode_char`,
  `BCRYPT_ALPHABET.bytes()` внутри `alphabet_char`) при промоушене волны std/_experimental→std
  2026-07-08. Минимальная репродукция:
  ```nova
  const ALPHABET str = "abcdef"

  fn count_a() -> int {
      mut n = 0
      for c in ALPHABET.chars() {          // .chars() ПРЯМО на const
          if c == 'a' { n += 1 }
      }
      n
  }
  // вызов из test-блока → CC-FAIL:
  //   initializing 'NovaValue_CharsIter' with an expression of incompatible type 'int'
  //   (nova_fn_ALPHABET_chars() эмитится вместо строкового instance-метода)
  ```
  **Обход в `bcrypt.nv`:** `ro alphabet = BCRYPT_ALPHABET` (локальная ro-копия) ПЕРЕД вызовом
  `.chars()`/`.bytes()` — на локальной переменной резолвится корректно. Тело задачи: резолвер
  member-call на identifier должен ПЕРВЫМ делом проверить, не является ли identifier
  const-значением (тогда instance-метод на его типе), и только потом рассматривать
  path-call-интерпретацию (`Name.method()` как namespace-call); либо, симметричнее, path-call-
  интерпретация должна требовать, чтобы `Name` резолвился в TYPE, не в `const`-binding.

- **[M-c-keyword-ident-collision]** (2026-07-08, P2, Wave: 172.12-хвост или Plan 185) — Nova
  identifier, совпадающий с зарезервированным словом C (`long`, и по тому же классу риска —
  весь стандартный список C89/C99 keywords), ломает codegen: эмиттер выводит имя переменной
  as-is в C без манглинга/экранирования. Обнаружен в `crypto/bcrypt.nv` (`mut long = ""` —
  тестовая переменная-пароль) при промоушене волны std/_experimental→std 2026-07-08.
  Минимальная репродукция:
  ```nova
  fn concat_pw() -> str {
      mut long = ""                         // `long` — C keyword
      for i in 0..3 { long = long + "a" }
      long
  }
  // вызов из test-блока → CC-FAIL:
  //   'long type-name' is invalid
  //   expected identifier or '('
  ```
  **Обход в `bcrypt.nv`:** переименована в `long_pw`. Тело задачи: эмиттер обязан манглить/
  экранировать ЛЮБОЙ user-identifier, совпадающий с зарезервированным C-словом, ДО вывода в C
  (напр. суффикс `_nv` или префикс, единообразно для var/field/param/fn-имён) — пользователь
  Nova не должен знать о зарезервированных словах ЦЕЛЕВОГО языка кодогена. Список C89/C99/C11
  keywords для покрытия: `auto`, `break`, `case`, `char`, `const`, `continue`, `default`, `do`,
  `double`, `else`, `enum`, `extern`, `float`, `for`, `goto`, `if`, `inline`, `int`, `long`,
  `register`, `restrict`, `return`, `short`, `signed`, `sizeof`, `static`, `struct`, `switch`,
  `typedef`, `union`, `unsigned`, `void`, `volatile`, `while`, `_Bool`, `_Complex`, `_Imaginary`
  (+ C11 `_Alignas`/`_Alignof`/`_Atomic`/`_Generic`/`_Noreturn`/`_Static_assert`/`_Thread_local`).

- **[M-static-selfreturn-value-mangle-conflict]** (2026-07-08, P2, Wave: 172.12-хвост/172.13) —
  ЛЮБАЯ static-namespace функция `fn Type.method(...) -> Self` (или `-> Result[Self, E]`) для
  **named-tuple/value-record** типа (D215/D228, `NovaTuple_<Type>`/`NovaValue_<Type>`-
  представление), СОСУЩЕСТВУЮЩАЯ в одном compile unit с instance-методами (`fn Type @method`)
  того же типа, даёт CC-FAIL: `conflicting types for 'Nova_<Type>_method_equal'` +
  `returning 'NovaTuple_<Type>' from a function with incompatible result type
  'NovaTuple_<Type> *'` (форвард-декларация статик-конструктора помечает возврат ПО УКАЗАТЕЛЮ,
  тело эмитит возврат ПО ЗНАЧЕНИЮ — рассинхрон двух путей регистрации типа для одного и того же
  `Type`). Уточняет/апгрейдит старую запись «`std/_experimental/math/complex` CC-FAIL —
  pre-existing (`NovaTuple_Complex`/`Nova_Complex_method_equal` mono gap)» — root-cause был
  неизвестен; теперь изолирован эмпирически (волна std/_experimental→std, 2026-07-08).
  **НЕ специфично для Result-wrapping** (`[M-181-result-over-named-tuple-codegen]`,
  ✅ RESOLVED 2026-06-26, чинил ИМЕННО Result[T,E]-обёртку над named-tuple — другой, уже
  закрытый путь) — воспроизводится и для голого `-> Self`. **НЕ специфично и для конкретных
  имён `.new`/`.from`** — попытка убрать `Complex.new`/`Complex.from`/`Complex.from_imag`
  (тривиальные обёртки) НЕ разблокировала файл, т.к. `Complex.from_polar`/`Complex.try_from`
  (обе тоже static-namespace, `-> Self`/`-> Result[Self,E]`) остаются и тоже триггерят класс.
  Минимальная репродукция (named tuple + один static-конструктор + один instance-метод —
  этого достаточно, `try_from` необязателен для триггера, но подтверждает что Result-форма
  туда же):
  ```nova
  export type Pt(x f64 = 0.0, y f64 = 0.0)

  export fn Pt.new(x f64, y f64) -> Self => Pt(x, y)     // static, -> Self
  export fn Pt @plus(other Pt) -> Pt => Pt(@x + other.x, @y + other.y)  // instance-метод

  test "basic" {
      ro p = Pt.new(1.0, 2.0)
      ro q = p.plus(Pt.new(3.0, 4.0))
      assert(q.x == 4.0)
  }
  // CC-FAIL:
  //   returning 'NovaTuple_Pt' from a function with incompatible result type 'NovaTuple_Pt *'
  //   passing 'NovaTuple_Pt *' to parameter of incompatible type 'NovaTuple_Pt'
  ```
  Без static-конструктора (только bare `Pt(x, y)` record-литералы на call-site + `@plus`) —
  компилируется чисто; без instance-метода (только static-конструктор) — тоже чисто. Нужны
  ОБА для триггера. Тело задачи: унифицировать регистрацию C-представления `Type` между
  static-namespace-функциями (`Type.method`) и instance-методами (`Type @method`) для
  named-tuple/value-record — сейчас, по всей видимости, это два разных code path в
  `emit_c.rs`, каждый решающий pointer-vs-value независимо. Не устранено воркэраундом на
  уровне `.nv` (кроме полного отказа от static-namespace конструкторов для таких типов, что
  неприемлемо — теряет читаемый API); `math/complex.nv` остаётся в `_experimental`, вне
  периметра волны std/_experimental→std 2026-07-08.

- **[M-lint-phantom-unused-vec-import]** (2026-07-08, P3, lints.rs) — `nova check` на КАЖДОМ
  std-файле репортит `warning: unused import 'Vec' — imported but never referenced [unused-import]`
  с span'ом, указывающим в произвольный текст КОММЕНТАРИЯ (напр. `std/checksums/crc32.nv:15:3` —
  середина слова в doc-комменте), при том что файл НЕ содержит ни одного `import`-стейтмента
  вообще (подтверждено грепом; 43+ shipped-файлов std до волны 2026-07-08 — `std/net/addr.nv`,
  `std/collections/hashmap.nv`, вся `std/time/` и т.д.). Фантом: `is_prelude_import`
  (`lints.rs:665`) вайтлистит только импорты с path-префиксом `std.prelude`, но какой-то
  инжектируемый prelude/auto-import Vec-имени проходит с другим path и bogus-span. Ложный
  сигнал на всём дереве std → делает недостижимым гейт «0 WARN» для любого std-файла и
  обесценивает unused-import lint целиком (шум маскирует настоящие unused). Тело задачи:
  найти источник синтетического импорта `Vec` (resolve_imports inline-инжекция?), пометить
  его synthetic-флагом и скипать в `lint_unused_imports`, либо расширить `is_prelude_import`
  на его реальный path. Волна промоушена 2026-07-08 репортила гейт как «PASS + 1 phantom-WARN
  на файл» со ссылкой сюда.

- **[M-hashmap-swisstable-candidate]** (2026-07-08, P3-кандидат, Plan: отдельный атом ПОСЛЕ
  всех текущих работ (директива владельца); Wave: [opus-карта → sonnet]) — апгрейд
  внутренностей std/collections/hashmap.nv на SwissTable-раскладку: контрольные байты
  (1Б/слот: Empty/Deleted/H2-отпечаток 7 бит) отдельным []u8 + KV-массив без enum-тегов;
  групповое пробирование по 16 (портативный u64-путь, SIMD опционально); load factor 7/8;
  Deleted только при полной группе (минимум могилок). Публичная поверхность НЕ меняется.
  Поля уже закрыты D281-формой `priv { }` (2026-07-08) — layout свободен для замены.

- **[M-d162-structural-throw-sibling]** (2026-07-08, P2, Plan: 172.13 чекер-каналы;
  Wave: с остальными чекер-маркерами; **ЗАКРЫТ батчем 3**) — D162 (uncovered-error-path)
  — структурный, не
  dataflow: требует throw ПРЯМЫМ сиблингом сразу после потребления; throw внутри
  match-ветки (`Err(_) => throw`) при наличии раннего return в той же fn не доказывается,
  даже когда поток очевидно безопасен. Обходной канон (применён в _experimental
  encoding/toml, text/regex, волна 3б 2026-07-08): `if x.is_err() { throw ... }` сиблингом
  + отдельный match для извлечения (Err-ветка = panic("unreachable")). Правильный фикс:
  научить D162 покрытию через match-ветки (или полноценный dataflow по путям).
  **Фикс (батч 3):** throw-скан D162 (`expr_has_throw`, types/mod.rs) спускался
  только в If/Block/With — расширен на Match-ветки (Expr и Block тела), IfLet,
  While/WhileLet/For/Loop. Обход `is_err()` снят в std/text/regex.nv
  (parse_quantifier_max → естественный match). В _experimental/encoding/toml.nv
  is_err-обходов НЕ оказалось (его err-Option-аккумуляция в parse_basic_string —
  D133-мотивированная, не D162). Позитив добавлен в
  spec_tests/conformance/d162_consume_defer_cover.nv (throw в match-ветке =
  покрытие). Гейты: conformance 67/0, std/text зелёный.

- **[M-redundant-param-ro-diagnostic]** (2026-07-08, P2, Plan: 172.13/185;
  **ЗАКРЫТ батчем 4**) — вопрос владельца: `fn f(bytes ro []u8)` — избыточный явный `ro`
  в позиции параметра принимается молча. D246 мандатит redundancy-ошибку только для
  указателей (`*ro T` → E_REDUNDANT_POINTER_RO), параметровая позиция не покрыта, хотя
  принцип тот же (параметры = ro-вид по умолчанию). Сделать зеркальную диагностику
  E_REDUNDANT_PARAM_RO (или W_ на переходный период) + амендмент-строку в D246.
  Обе синтаксические позиции: type-modifier `bytes ro []u8` И префикс-режим
  `(ro bytes []u8)` — диагностика должна крыть обе. 8 сайтов в std вычищены
  рукой 2026-07-08 (chars.nv, core.nv ×2 + from_bytes-тройка, bcrypt.nv,
  string_builder.nv @append).
  **Реализация (батч 4):** hard error E_REDUNDANT_PARAM_RO в parse_param
  (обе формы; гейт `!is_mut` сохраняет V3-комбо `ro x mut T` легальным) +
  hard error E_REDUNDANT_RETURN_MUT на `-> mut T` (top-level only —
  `-> *mut T` не задет). Амендмент-строка в D246 (02-types.md, рядом с
  таблицей E_REDUNDANT_POINTER_RO). Neg: d246_redundant_param_ro_prefix_neg,
  d246_redundant_param_ro_type_neg, d246_redundant_return_mut_neg; позитив-
  граница d246_param_ro_mut_view (V3-комбо компилируется и исполняется).
  Греп std/spec_tests/examples: остаточных сайтов 0 (один легитимный ранее
  существовавший `b ro []u8` в d176_ro_type_modifier.nv переписан на голую
  форму с примечанием об амендменте). Conformance 70 файлов (67+3 neg),
  единый-CU прогон PASS.

- **[M-runtime-folder-run-ice-vec-ident]** (2026-07-08, P2, Plan: 172.13, родня
  [M-per-file-check-no-prelude-protocol-scope]) — `nova test --full std/runtime`
  папкой = довливной ICE emit_c.rs:44410 «Ident `Vec` not in var_types» (проверено
  на чистом 3b74fff01 тем же бинарём). Пофайловые прогоны runtime-модулей зелёные;
  папочная агрегация #no_prelude-CU теряет тип Vec. В гейты приёмок runtime папкой
  не входил — класс вскрыт при unsafe-волне from_bytes_unchecked.

- **[M-option-self-recursive-record-mono]** (2026-07-08, P1; **ЗАКРЫТ 2026-07-10,
  Plan 186 recursive-mono, ветка recursive-mono**) — самоссылочный рекорд
  `type Node { value int; next Option[Node] }` мис-мономорфизировался: поле
  `next` эмитилось `NovaOpt_nova_int` (тип ПЕРВОГО поля) вместо
  `NovaOpt_Nova_Node_p`. Корень: `record_schemas`/`sum_schemas` регистрируют
  схему типа только ПОСЛЕ разбора всех его полей — самоссылающееся поле
  резолвилось, пока охватывающий тип ещё «невидим» реестрам, и ошибочно
  классифицировался как нерезолвленный generic-стаб (эрейзится в nova_int).
  Второй, более узкий инстанс того же корня: структурная eq-регистрация
  `Option[Self]` (`register_novaopt_decl`) срабатывала eagerly (побочный эффект
  вычисления C-типа поля) и по той же причине не видела схему — молча
  деградировала до pointer-identity `==` (structurally-equal-но-раздельно-
  аллоцированные цепочки сравнивались как НЕ равные). Фикс: новый guard
  `being_defined_record_types` (зеркало существующего `being_defined_sum_types`)
  помечает тип конкретным, пока эмитятся его собственные поля — консультируется
  БЕЗУСЛОВНО (не только при `full=true`, как старый sum-guard: ровно тот сайт,
  что ловит баг — `Option`'s inner-type check в `resolved_named_to_c` — зовёт с
  `full=false`); eq-регистрация теперь ОТКЛАДЫВАЕТСЯ
  (`pending_structural_eq_bodies`, дренится сразу после эмиссии всех
  не-generic type-деклараций). Родня [M-result-direct-recursive-enum] — тот же
  класс «рекурсивный композит в generic-контейнере», закрыт той же волной.
  Позитив-фикстура: `nova_tests/recursive_mono/pos/option_self_linked_list.nv`
  (round-trip чтения/записи + структурное `==` на раздельно-аллоцированных
  равных цепочках). compiler-codegen/src/codegen/emit_c.rs.

- **[M-property-testing-rot]** (2026-07-08, P2, Plan: 172.13 — юнификация; Wave: с
  чекер-маркерами; **ЗАКРЫТ батчем 3** — std/testing полностью зелёный, turbofish снят)
  — std/testing/property.nv никогда не гонялся гейтами и сгнил слоями.
  Сняты лично 2026-07-08: mut-sort на ro-биндингах (D36), str.len()→byte_len() (D249),
  push-петли→append-срезы (§18а), turbofish на 5 вызовов property[T]. ОСТАЛСЯ корень:
  протокольная юнификация — `property[[]int](gen ArrayGen[int], ...)` резолвит T=int
  (по тип-аргументу конструктора gen), игнорируя структурную реализацию
  Generator[[]int] через `ArrayGen[T] @generate() -> []T`; ресивер лямбды типизируется
  int → E_PRIMITIVE_NO_PROTOCOL_METHOD sort. Родственно исходному «cannot infer T».
  std/testing добавить в гейты после фикса.
  **Разбор батча 3 (слои, все закрыты):** (0) дизайн-корень контента: протокол как
  ТИП значения (`gen Generator[T]` параметр / `ro elem Generator[T]` поле) — у
  протоколов Nova НЕТ runtime-диспетча (D53), такие вызовы эмитились NULL;
  property.nv переписан на канон bounded generics
  (`property[G Generator[T], T](gen G, ...)`, `type ArrayGen[G Generator[T], T]`,
  статический диспетч — прецедент `Vec@extend[S Iter[T]]`). Компиляторные каналы
  (emit_c.rs): (1) mono-путь generic-вызова эмитил closure-аргумент fn-типизированного
  параметра БЕЗ контекста → параметры лямбды дефолтились в nova_int — теперь
  substituted-сигнатура передаётся в emit_lambda; (2) структурная протокольная
  юнификация `infer_protocol_structural_binding` (Case A mono-инстанс через
  generic_type_instance_info+шаблонные методы, Case B не-generic тип через
  method_overloads; глубина ограничена — протоколы в позициях протоколов);
  подключена в infer_type_param_binding (`_`-ветка), в Source 2e-bis
  resolve_mono_type_args (баунды fn-generics), в infer/emit static-ctor каналы
  (infer_generic_static_ctor_ret + try_generic_static_ctor_mono — вывод T из
  ТИП-уровневого баунда `[G Generator[T], T]`); (3) Source 2f: вложенный
  generic-вызов в mono-теле, пробрасывающий параметры объемлющей fn —
  TypeRef-юнификация через новый current_fn_param_typerefs
  (+infer_type_param_binding_from_ref); (4) subst_map_adopt_rt адоптил
  самоссылочный RT (`T → Named{T}`, байт-гейт проходит под подстановкой
  вызывающего) → бесконечная рекурсия lowering — guard mentions_slot;
  (5) infer-двойник generic-return: erased `fn_ret_<name>` больше не
  перехватывает mono-вывод (stash-фолбэк), и `(T, UserType)`-возвраты
  лоуэрятся через type_ref_to_c под overrides (apply_type_subst_to_ref
  не знал user-типов в элементах); (6) чекер: passthrough-тайпвар объемлющей
  fn больше не «не удовлетворяет баунду» (current_fn_generic_names, D72
  энфорсится на конкретных колл-сайтах); (7) **GC-корень (главная находка):**
  Boehm НЕ сканирует Windows TLS — handler, живущий ТОЛЬКО в
  `_nova_handler_<eff>` (форма `with Random = th.seeded(42)` инлайнила вызов
  фабрики прямо в TLS-присваивание), собирался коллекцией (~32 итерации
  generate+clone, детерминированный segfault) → use-after-free; emit_with
  теперь ПИНИТ значение хендлера в stack-локале на время блока (консервативный
  скан стека держит vtable+ctx+замыкания). Контент property.nv: BoolGen
  получил поле p_percent (пустой record-литерал не поддержан грамматикой —
  единственный пустой record в std), Iter[T]→[]T в shrink (протокол в
  return-позиции), формы конструкторов. std/testing в гейтах, 6/6 тестов.

- **[M-rawmem-typed-copy-wrappers]** (2026-07-08, P2, Plan: 172.13 — generic-mono;
  Wave: после фикса корня) — предложение владельца: типизированные
  `RawMem.copy_n[T]/copy_n_nonoverlapping[T](src *T, dst *mut T, count int)` (счёт в
  элементах, Rust ptr::copy<T>) вместо кастов+`* size_of[T]()` в точках вызова.
  ЗАБЛОКИРОВАНО компиляторным гэпом: чистая .nv generic-обёртка с *T-параметрами,
  вызванная из generic-метода (`Vec[T] @cap` → wrapper), мискомпилится — d141-фикстура
  падала рантаймом «index 5 out of bounds for length 1» (T=u8 путь); откат вызова на
  каст-форму лечит. Две заметки реализации: (1) перегрузка одним именем с байтовым
  extern невозможна — коллизия при T=u8, имена copy_n*; (2) репро = вернуть обёртки
  и вызов в cap[T], conformance d141. Каст-форма в vec/core.nv — временный канон.

- **[M-exp-promotion-blockers]** (2026-07-08, P2-пакет, Plan: 172.13; Wave: батчами после
  первых 4 маркеров) — 6 компиляторных классов, блокирующих последние 6 модулей
  _experimental (детали и репро — в std/_experimental/STATUS.md, разметка волны 2):
  csv (nested [][]str runtime — **ЗАКРЫТ батчем 3, csv PROMOTED**), toml
  (Fail-handler mono gap Nova_*Error_p — ЗАКРЫТ
  батчем 3, см. ниже; НОВЫЙ блокер найден), url
  (tuple-destructure infer), uuid_namespace (duplicate-symbol md5+sha1 в одном CU —
  **ЗАКРЫТ батчем 3, uuid_namespace PROMOTED**: follow-up к дедупу 88a2ffe75 —
  квалифицированное имя коллизии теперь доходит и до FORWARD-декларации через
  mangle_fn; при этом регистрация в file_priv_fn_c_names сужена до коллизий с
  ИДЕНТИЧНОЙ сигнатурой — сигнатурно-различимые, как encode_with hex/base64,
  консистентно разруливаются overload-суффиксами D84, их квалификация ломала
  jwt_test),
  linkedlist (2× self-recursive generic mono — родня [M-option-self-recursive-record-mono]
  агента владельца), retry (E_UNUSED_PREFIX_TYPEVAR двусторонний). Плюс довливной
  вне пакета: std/time/timer_metrics_test CC-FAIL (NovaValue_Timestamp ← int,
  воспроизведён на c65af77ed) — папка time впервые в гейтах.

- **[toml Fail-mono, батч 3]** — корень: mono-name mangling конвенция кодирует
  pointer-typed T как суффикс `_p` в идентификаторе (`*` не ident-safe) —
  `Option[ParseTomlError]` (heap sum-type) моно-имя `NovaOpt_Nova_ParseTomlError_p`.
  `Option[T].unwrap()`'s return-type inference (`infer_expr_c_type`, ДВЕ
  дублированные копии, emit_c.rs ~44460/~48188) брала extracted-суффикс
  НАПРЯМУЮ как C-тип вместо реверса `_p`→`*` — `throw err.unwrap()`
  (toml.nv:287) получал bogus C-тип `Nova_ParseTomlError_p` (не объявлен) →
  CC-FAIL «unknown type name». Фикс: новый helper
  `debt_unmangle_ptr_suffix` (emit_c.rs), применён в обеих копиях (третья
  копия того же паттерна, emit_c.rs:28667, НЕ трогать — там elem_ty
  используется для построения ДРУГОГО mangled-имени, где `_p` обязан
  остаться). Конформанс 67/0 без регрессий; std/encoding, std/data чисты.
  **Hoist-блокер ЗАКРЫТ 2026-07-10 (Plan 186 recursive-mono)** —
  `[M-toml-sum-variant-mono-field-hoist]`: `emit_sum_type`/`emit_record_type`
  писали `struct Nova_{name} { ... }` без pre-pass форвард-декларации для
  pointer-полей, чьё C-имя — mono'd generic instance ещё не эмитированный
  (typedef только после `drain_generic_type_worklist`). Фикс — pre-pass
  (собрать все field-типы ДО открытия struct, эмитить `typedef struct X X;`
  для `Nova_`-префиксных pointer-типов) в ОБЕИХ функциях, зеркалит уже
  существующий паттерн `emit_generic_type_instance`'s Record-ветки.
  `being_defined_sum_types`/`being_defined_record_types` (см.
  [M-option-self-recursive-record-mono] выше) переставлены на самый ВЕРХ
  функции — pre-pass тоже зовёт `type_ref_to_c` на своих полях, гвард
  обязан быть виден и на этом первом проходе. toml.nv теперь компилируется
  И линкуется. compiler-codegen/src/codegen/emit_c.rs.
  **`[M-toml-repeated-fail-call-run-fail]` ЗАКРЫТ 2026-07-10** (ветка
  toml-fail-fix, sonnet) — исходная гипотеза («runtime/codegen баг в
  повторном использовании Fail-эффект-хендлера / consume-биндинга внутри
  ОДНОГО with-скоупа») была НЕВЕРНОЙ. Расследование (минимальный репро БЕЗ
  toml, state-dump подход) показало: механизм Fail-frame/`with Fail[E]`/
  D65-диспатч — не сломан; ПОВТОРНЫЕ вызовы Fail-эффектной функции в одном
  with-скоупе работают корректно (закреплено новыми pos-тестами в
  `std/encoding/toml_test.nv`). ПЕРВОНАЧАЛЬНОЕ наблюдение «4/6 тестов
  FAIL, 2 PASS» само оказалось артефактом: `nova test-build`/`nova test`
  усекают detail-вывод до подмножества FAIL-строк — реальный прогон на
  НЕИСПРАВЛЕННОМ toml.nv давал 0/6 (проверено прямым запуском
  собранного .exe в обход CLI-обёртки).
  Настоящий корень — ДВА независимых, чисто локальных бага в САМОМ
  toml.nv, не связанных с Fail вообще:
  1. `is_bare_key_char`'s многострочная `||`-цепочка использовала ВЕДУЩИЙ
     `||` на каждой продолжающей строке. `||` — ОДНОВРЕМЕННО синтаксис
     zero-arg closure-литерала (`|| body`); парсер (`parse_or`,
     compiler-codegen/src/parser/mod.rs) сознательно НЕ распространяет
     newline-tolerance на ведущий `||` (во избежание мисparse настоящего
     `|| body` как продолжения OR-цепочки предыдущей строки). Каждая
     ведущая-`||` строка молча становилась ОТДЕЛЬНЫМ discarded zero-arg
     closure-литерал-statement'ом; итоговое (trailing) значение функции
     оказывалось указателем НА ПОСЛЕДНИЙ closure, приведённым (coerced) к
     `nova_bool` — ВСЕГДА truthy, независимо от входного символа. Ни
     `nova check`, ни codegen не выдают диагностику (никакой ошибки) —
     заведён ОТДЕЛЬНЫЙ follow-up-маркер `[M-closure-trailing-scalar-
     coercion-no-typecheck]` (checker должен отвергать closure-типизиро-
     ванное trailing-выражение против скалярного return-типа; не чинился
     в этой волне — компилятор-уровня фикс несоразмерен точечной toml-
     задаче). Фикс: перенести `||` в КОНЕЦ каждой строки (trailing-
     оператор перед newline — продолжение БЕЗ closure-неоднозначности).
  2. `@parse_number` вызывал РЕТРАКТИРОВАННЫЙ `f64.try_from`/
     `i64.try_from(str)` ([M-f64-try-parse-to-parse-f64], Plan 174.1,
     известно-сломан — `f64.try_from("3.14")` молча возвращает `3.0`).
     Фикс: канон `str @to_f64()`/`str @to_i64()`
     (std/runtime/string/parse.nv).
  Оба репродуцированы В ИЗОЛЯЦИИ (минимальный `is_bare_key_char`-подобный
  фн без toml/Fail/consume; прямой вызов `f64.try_from("3.14")`),
  подтверждая отсутствие связи с Fail-повторами. toml PROMOTED в
  `std/encoding/` (git mv + peer `toml_test.nv`, конвенция w2/batch3).
  Гейты: cargo build чисто; conformance 90/0; err173-корпус δ0; toml
  peer-тесты (9: 6 исходных + 3 новых pos-регресс на repeated-Fail-call)
  PASS `test --full`; `nova check std/` δ0. См.
  std/_experimental/STATUS.md «PROMOTED 2026-07-10» для полного разбора.

- **[M-generic-bound-forwarding]** (2026-07-08, P2, Plan: 172.13 батч 4;
  **ЗАКРЫТ батчем 4** — заведён по факту и закрыт той же волной) — bound не
  проносился через вызов bounded-generic-fn из bounded-generic-fn
  (`fn outer[R Read](r mut R) => inner(r)` — «R does not satisfy the bound»),
  задокументировано владельцем в std/io/core.nv:12-15 («the checker does not
  yet carry a bound through such a forward») — из-за чего петли read/write
  были ИНЛАЙНЕНЫ в каждый хелпер (5 копий). КОРЕНЬ уже был закрыт побочно
  батчем 3 (fix `[M-property-testing-rot]` слой (6): passthrough-тайпвар
  объемлющей fn удовлетворяет баунд через `current_fn_generic_names`, D72
  энфорсится на конкретных колл-сайтах; коммит 46645224a) — батч 4 подтвердил
  эмпирически (одно- и двухслойный форвард компилируются и исполняются) и
  СХЛОПНУЛ инлайн-копии: read_to_string/lines/byte_lines → read_to_end,
  write_str → write_all (однострочник), copy's внутренняя write-петля →
  write_all(chunk.first_n(rn)). Наблюдаемая дельта: context-строка WriteZero
  из write_str/copy теперь "write_all" (общий хелпер), не имя обёртки — тестов
  на context-строку не было. Позитив-пин: spec_tests/conformance/
  d122_generic_bound_forwarding.nv (1 и 2 слоя форварда, конкретный вызов,
  runtime-ассерты). Гейты: std/io зелёный, std/net зелёный (потребитель
  write_all), conformance PASS.

- **[M-c-keyword-mangle-destructure-tail]** (2026-07-08, P3, Plan: 172.13 хвост;
  **ЗАКРЫТ батчем 4**) — манглинг C-keyword идентификаторов (закрыт батчем 1,
  f8db4abbe) НЕ распространён на tuple/record-destructure bind-имена и пары
  `if x is T`-narrowing / defer-capture — сайты не задеты репро батча 1, честно
  зафиксированы автором. Довести теми же каналами + расширить
  conformance/c_keyword_ident_mangling.nv этими позициями.
  **Реализация (батч 4):** тем же каналом `mangle_field_name` (text-only,
  var_types на raw-имени) закрыты 7 декларационных сайтов emit_c.rs:
  emit_tuple_destructure ×3 (Channel.new-пара, direct-pairing `= (a,b)`,
  general tmp-struct путь), pattern_bind_typed Tuple-Ident,
  pattern_bind_typed Record ×4 (plain shorthand/renamed + sum-variant
  shorthand/renamed). `if x is T` (keyword-именованный scrutinee) и
  defer-capture keyword-локала эмпирически оказались УЖЕ покрыты
  батчем 1 (Ident-fallback чтения + обычный let-путь деклараций) —
  пиннированы тестами. conformance/c_keyword_ident_mangling.nv расширен
  позициями 5-8 (5 новых тестов), PASS.

- **[M-random-u64-path-return-ice]** (2026-07-08, P2, Plan: 172.13 батч 3;
  **ЗАКРЫТ батчем 3**) — тупик батча 2:
  `Random.u64()` внутри Uuid.v4()/v7() (транзитивно) = ICE «Path call return type unknown»;
  воспроизведён на baseline d987de52d и на уже промоутнутом std/identifiers/uuid.nv
  сам-по-себе — довливной. Блокирует промоушен uuid_namespace (его собственный
  dup-symbol корень ЗАКРЫТ батчем 2, 88a2ffe75).
  **Корень (батч 3):** `Random` — единственный ambient-эффект, объявленный НЕ
  в prelude, а в `std/testing/handlers.nv`; CU модуля, не импортирующего
  testing.handlers (uuid/ulid/retry сами по себе), не имел
  `effect_schemas["Random"]` → return-тип effect-op'а неизвестен → ICE.
  **Фикс:** декларация `export type Random effect { u64() -> u64; bytes(n int)
  -> []u8 }` перенесена в `std/prelude/effects.nv` (прецедент D316 — prelude =
  единственный источник схемы; в отличие от Time, vtable Random эмитится
  codegen'ом из декларации — обычный user-effect путь). В handlers.nv осталась
  только фабрика `seeded()`. Компиляторный код не менялся. Гейты: conformance
  67/0; std/identifiers, std/testing/handlers, std/concurrency, std/crypto,
  std/time (кроме задокументированного довливного timer_metrics_test) зелёные.

- **[M-consume-rebind-nested-block-shadow]** ✅ **CLOSED 2026-07-08** (найден батчем 2,
  **зафиксирован ФИКСОМ в ТОМ ЖЕ дне** коммитом `3f0198c8fd`, Plan 172.13 батч 3) —
  `consume x = StringBuilder.new()` РЕ-БИНД внутри вложенного if/else эмитился как НОВОЕ
  C-объявление, тенющее внешнее только на блок — после выхода итерация цикла видела
  старую (consumed) переменную. Чекер молчал; семантика D347 ожидает поведение
  mut-переприсваивания. Фикс: `alpha_rename` различает same-scope rebind (existing path)
  vs rebind, чья прежняя привязка живёт в ОХВАТЫВАЮЩЕМ scope (новое: имя не трогается,
  span стейтмента пишется в `Module::consume_reuse_spans`); `emit_c.rs` при виде спана
  эмитит plain reassignment вместо fresh block-scoped C-декларации (тот же `is_hoisted`
  канал). Регресс: `spec_tests/conformance/d347_same_scope_rebinding.nv` (2 новых теста,
  включая loop-shaped repro один-в-один как в этом маркере). **Ретроактивная
  верификация 2026-07-10 (Plan 173 P1-волна):** маркер оставался помечен OPEN в этом
  файле по документационному долгу — код-фикс уже был в истории HEAD задолго до волны.
  Перепроверено на слиянии: `d347_same_scope_rebinding` PASS (1/0), `std/encoding/csv_test`
  PASS (1/0). Блокировавший csv Vec-LHS корень закрыт отдельно (6bf62be48).

- **[M-ffi-handle-newtype]** (2026-07-09, P3, Wave: после лимитов) — решение владельца:
  FFI-хендлы не гуляют по модулю голым int/указателем — заворачивать в типизированную
  обёртку (net-образец TcpListener{priv handle}). СДЕЛАНО: BrotliDec в
  std/encoding/compress/brotli.nv (int только на extern-границе внутри методов).
  ОСТАТОК: fs scandir — хендл пересекает Fs-эффект (vtable носит примитивы),
  обёртка требует value-record слотов в эффект-vtable либо net-канона
  (*()+out_err) в самих шимах; net internal plumbing (*() между priv-функциями
  ffi.nv) — довернуть при том же заходе.

- **[M-newtype-receiver-method-dispatch]** (2026-07-09, P2, Plan: 172.13-хвост/батч 4+;
  Wave: с чекер-маркерами) — методы на newtype-ресивере (`type BrotliHandle(int)`;
  `fn BrotliHandle @feed(...)`) не регистрируются под своим типом: вызов `dec.feed(...)`
  диспатчится ПО ИМЕНИ на чужой `ZlibWriter @feed` того же CU (too many arguments в C).
  Класс coarse-by-name. Репро: std/encoding/compress/brotli.nv вернуть методы на
  BrotliHandle из истории коммита. Сейчас в brotli — прямые вызовы типизированных
  extern'ов (типобезопасность хендла сохранена).

- **[M-result-direct-recursive-enum]** (2026-07-09, **P1 — компилятор жрёт 8-17 ГБ/виснет**;
  **ЗАКРЫТ 2026-07-10, Plan 186 recursive-mono, ветка recursive-mono**) —
  `Result[X, E]`, где heap-enum X имеет прямо-рекурсивный вариант (tuple-,
  record- и `[]X`-Vec-формы), детерминированно валил компилятор аллокацией
  (наблюдалось 6.6+ ГБ за 60с и продолжало расти на минимальном 11-вариантном
  репро). Корень (emit_c.rs `emit_field_eq`): регистрация `Result[X,E]`
  (`register_novares_decl`) попутно регистрирует `Option[X]`/`Option[E]` (под
  `.ok()`/`.err()`), что для heap sum/record X уходит на СТРУКТУРНУЮ
  генерацию `==` — та инлайнила ПОЛНОЕ per-variant/per-field сравнение на
  КАЖДОМ уровне вложенности для самоссылающегося поля (ограничено только
  общим `MAX_EQ_DEPTH=32`) → `O(branching^depth)` рост строки. Фикс:
  `struct_eq_stack` отслеживает типы, разворачиваемые ПРЯМО СЕЙЧАС; настоящий
  цикл (тип встречен повторно) обрывает инлайнинг и уходит на ИМЕНОВАННУЮ,
  единожды эмитируемую функцию `nova_struct_eq_<T>` — самоссылающееся поле
  есть реальный C heap-указатель, поэтому обычный рекурсивный ВЫЗОВ ФУНКЦИИ
  даёт C-компилятору/рантайму обработать фактическую (конечную) глубину
  данных вместо развёртки на этапе компиляции. Нецикличные типы byte-for-byte
  не затронуты. Родня [M-option-self-recursive-record-mono] — тот же класс
  «рекурсивный композит в generic-контейнере», закрыт той же волной.
  Позитив-фикстура: `nova_tests/recursive_mono/pos/enum_tree_result.nv`
  (11→3-вариантный самоссылающийся enum через Result, структурное `==` на
  bare/nested/Vec-вариантах). compiler-codegen/src/codegen/emit_c.rs.

- ✅ **[M-lazy-const-init-race]** (2026-07-09, **P1 — UB-гонка в M:N**, вопрос владельца;
  ✅ ЗАКРЫТ (проверено 2026-07-11: nova_consts_init() реализована в emit_c.rs);
  Plan: волна сразу после 174.1, зона emit_c const-канал; Wave: [sonnet]) —
  генерируемый lazy-init модульных констант (`nova_const_X(void)`: check-then-act по
  неатомарному `_init`-флагу, без барьеров) не потокобезопасен: двойной gc_add_root,
  а главное — публикация value без release/acquire (тред видит init=1 раньше value;
  для указательных констант nova_str/Vec = крэш-класс, родня fs-TLS). ФИКС: снять
  ленивость — кодоген собирает инициализаторы в одну `nova_consts_init()`
  (топологический порядок по зависимостям), вызов из драйвера ДО спавна воркеров;
  чтение констант становится голым значением (минус ветка на каждом доступе).
  Атомики не нужны. Пример: nova_const_ADDR_IMAGE_BYTES в любом net-CU.

- **[M-lint-findings-static-conversion]** (2026-07-09, P3, Wave: миграционная волна §1а;
  источник: план 185, `nova lint std/`) — 21 сайт статик-конверсий `T.from(x)` /
  `T.parse(s)` в std (Csv/Ini/Json/**Toml**.parse, Url/Body/BodyReader/HeaderValue/Ulid/Uuid.from,
  HashMap.from, ~~Vec[T].from~~, JsonValue.try_from и др.) — «пятая дверь» по §1а
  nv-coding-style (ретракция 2026-07-09). Миграция = переименование публичного API
  (`s.to_json()`-семья) + правка вызовов; не входит в план 185. On-line маркеры
  на декларациях; полный список — `nova lint --rule W_STATIC_CONVERSION std/`
  после снятия маркеров. [lint-sanitation 2026-07-10]: `Toml.parse` добавлен в
  список — промоушен toml.nv из std/_experimental случился ПОСЛЕ первоначальной
  волны (2026-07-09), маркер не был проставлен; проставлен сейчас. `VersionReq.parse`
  (data/semver_range.nv) НЕ в этом списке — для него сделан полноценный фикс
  (`str @to_versionreq() -> Result[...]`, по образцу `semver.nv`), т.к. файл малой
  сложности без зависимых Fail-эффект regression-тестов. **`Vec.from`-часть ЗАКРЫТА
  Plan 200 П16 (2026-07-20)** — не переименование, а полный ретракт (владелец:
  «это же просто items.clone()»); декла снесена, все вызовы мигрированы на
  `.of(...)` (литерал) / `.clone()` (same-T) / явный цикл (width-конверсия).
  Маркер остаётся ОТКРЫТ для остальных 20 сайтов (HashMap.from и др.).

- **[M-lint-findings-fail-public-signature]** (2026-07-09, P3, Wave: D325-R5 миграция;
  источник: план 185) — 8 сайтов `Fail[XError]` в публичных std-сигнатурах
  (Csv/Ini/**Toml**.parse, Ulid.new/from, Uuid.from, testing/property assert_prop*) — по R5
  D325 канон `Result[T, XError]`. Миграция = смена сигнатур + вызовов (у property —
  осознанный throw-дизайн тест-ассертов, решить при заходе). Проверка:
  `nova lint --rule W_FAIL_PUBLIC_SIGNATURE std/`. [lint-sanitation 2026-07-10]:
  `Toml.parse` — доп. причина держать Fail-эффект (не просто «ленивый долг»):
  `std/encoding/toml_test.nv` (создан 2026-07-10, `[M-toml-repeated-fail-call-run-fail]`)
  ПИНИТ поведение ПОВТОРНЫХ Fail-эффектных вызовов в одном `with`-scope именно
  через `Toml.parse` как пробник механизма fail-frame; перевод на `Result`
  убрал бы единственный удобный Fail-эффектный вызов и обнулил регресс-покрытие
  того самого механизма. Конверсия — только вместе с ревизией этих тестов
  (не в периметре этой волны).

- **[M-lint-findings-manual-slice-copy]** ✅ **ЗАКРЫТ (2026-07-10, ветка std-hygiene).**
  ~~29~~ все сайты поэлементной копии `push(x[i])` в циклах (§18а) разобраны. (а)
  crypto/uuid — bulk-путь `[N]T`→`[]T` фикс-массива на самом деле УЖЕ существует:
  range-index `digest[0..N]` на `[N]T` работает и ВСЕГДА копирует (нет буфера для
  zero-copy view у фикс-массива), подтверждено runtime-экспериментом — заметка
  «нужен std/языковой примитив» ниже была устаревшей/неверной. (б) deflate/inflate/
  io/fs/http — честно нерегулярные пути (filtered-gather, conditional overwrite-or-
  append, per-position transcode) задокументированы inline-комментарием и false-
  positive снят локальной переменной вместо `push(x[i])`; regular contiguous-range
  копии заменены на срез-вид/`.append()`. `nova lint --rule W_MANUAL_SLICE_COPY std/`
  = 0 находок. Заодно снесены дублирующие `fn slice()`-обёртки в http/client+server/
  wire.nv и `head_slice()` в servernet.nv. Детали: docs/simplifications.md
  (2026-07-10 std-hygiene), коммит f2f7f65e2.

- ✅ **[M-lint-findings-writebuffer-into]** (2026-07-09, P3, Wave: вместе с D410-хвостом;
  источник: план 185) — ✅ ЗАКРЫТ (2026-07-17): `WriteBuffer consume @into() -> []u8`
  переименован в `@into_bytes()` (канон §1а), мигрировано 96 call-сайтов в spec_tests
  и std/src/encoding/url.nv. nova lint spec_tests: 103→7 findings.

- ✅ **[M-lint-findings-param-no-contract]** (2026-07-09, P3, Wave: контракт-волна §5;
  источник: план 185) — ✅ ЗАКРЫТ (2026-07-17): трём оставшимся сайтам без контракта —
  `HashMap[K, V].new(cap int = 16)`, `Queue[T].new(cap int = 0)`, `Set[T].new(cap int = 16)`
  (std/src/collections/{hashmap,queue,set}.nv) — дописан `requires cap >= 0` (владелец:
  "requires n >= 0 — ДА"; форма/имя параметра сверены с прецедентом `Vec[T].new(cap int = 0)
  requires cap >= 0`, std/src/collections/vec/core.nv). `nova check` трёх файлов чист;
  таргетные `nova test` (doctests hashmap/set, queue_test.nv) зелёные. nova lint std: 5→2
  находки (на момент этого закрытия; остальное закрыто соседним закрытием ниже).

- ✅ **[M-lint-findings-try-without-sibling]** (2026-07-09, P3, Wave: D325-R3 хвост;
  источник: план 185) — ✅ ЗАКРЫТ (2026-07-17), решён ДВУМЯ отдельными правками владельца:
  (1) `ReadFs`-протокол/`DirFs`/`EmbeddedDir` `@try_exists` (std/src/fs/readfs.nv) переименован
  в `@path_exists` (владелец: "path_exists — ДА"; сигнатура/`Result`-возврат не менялись) —
  call-сайты (readfs_test.nv, docs/io-fs.md), D323-амендмент `ReadFs` в
  spec/decisions/04-effects.md обновлены в том же слиянии; (2) `Duration.try_from_secs_f64`
  (static ctor, std/src/time/duration/core.nv) СНЕСЁН целиком (владелец: "мы убрали все
  Duration.from_*" — не exception к правилу, а снос статики), заменён ресиверной формой
  `f64 @checked_to_seconds() -> Option[Duration]` (зеркалит `@times(f64)`/`@checked_mul_f64`
  на `Duration`); call-сайты (core.nv inline-тест, spec_tests/conformance/
  d317_duration_overflow_policy.nv) мигрированы на `x.checked_to_seconds()`; D317-амендмент
  (R5 f64-конверсии) в spec/decisions/04-effects.md обновлён в том же слиянии. nova lint std:
  5→0 находок; nova lint spec_tests: 0 находок.

- **[M-lint-findings-result-discarded-lenient-parse]** (2026-07-09, P3;
  источник: план 185) — swallow-арм `Err(_) => ()` в lenient-парсерах
  (http/response_ext set-cookie, http/cookie max-age): пропуск невалидного элемента —
  осознанная семантика; задокументировать как канон-паттерн (§4) или ввести явный
  helper (`.ok()`-цепочка). On-line маркеры на месте.

  **ЗАКРЫТО (2026-07-09, worktree `nova-174`/`ptr-methods-174-5`, [sonnet]).**
  ~~генерируемый lazy-init модульных констант (`nova_const_X(void)`: check-then-act по
  неатомарному `_init`-флагу, без барьеров) не потокобезопасен~~ — снята ленивость:
  `emit_lazy_const` (emit_c.rs) больше не эмитит per-const getter — storage-only
  (`static Ty _nova_const_X_value;`, БЕЗ `_init`-флага), init-body собирается в
  `pending_const_inits` и на finalize (`emit_module`, перед `emit_main_wrapper`)
  объединяется в ОДНУ `static void nova_consts_init(void) {...}` (Kahn-топосорт по
  `collect_free_idents`-зависимостям, `topo_sort_const_inits`); вызов
  `nova_consts_init()` в `main()` сразу после `nova_gc_init()`, ДО
  `nova_runtime_auto_arm()` (спавн воркеров). Чтение констант — голое
  `_nova_const_X_value` (было `nova_const_X()`-call), два use-site (Ident +
  qualified-path) переведены. Атомики не понадобились (single-threaded init до
  конкурентности). Гейт: `nova test --positive --compile-error
  spec_tests/conformance` 78/0 (без регресса); `nova test std` 59/3(pre-existing,
  не регресс)/61-skip; спот-греп `.c` — `if (!_init)` веток 0, `nova_consts_init()`
  вызывается ровно один раз в `main()`; `std/net/addr_test` (ADDR_IMAGE_BYTES)
  PASS, спот-дифф `.c` подтверждает eager init + bare-value read.

- **[M-const-init-concurrency-gate]** (2026-07-09, P2, вопрос владельца; ✅ ЗАКРЫТ 2026-07-10
  волной 173.3 [sonnet]: `E_CONST_INIT_CONCURRENCY` в CapabilityCtx — spawn/detach/supervised/
  parallel-for/select/блокирующие `.send()`/`.recv()`/вызов Detach-fn в module-level ro/const
  инициализаторах; extern/runtime-вызовы легальны; D415 §6 + амендмент 03-syntax partition;
  фикстуры err173_3/neg/const_init_{spawn,supervised,detach_fn} + pos const_init_runtime_ok_test)
  — module-level ro-инициализаторы исполняются в
  nova_consts_init() ДО спавна воркеров, но чекер не запрещает в них конкуренцию
  (E_CONST_EFFECT_IN_INIT покрывает только const); _auto_arm_if_needed() лениво поднял бы
  M:N посреди consts_init → возврат гонки [M-lazy-const-init-race]. Фикс:
  E_CONST_INIT_CONCURRENCY (spawn/supervised/detach/parallel/каналы/Detach-эффект
  в сигнатурах вызываемых) + D215-амендмент. Сегодня спасает случайный гэп
  (supervised-value не поддержан в module-init) — не контракт.

- **[M-fixed-array-value-semantics] ПАУЗА-отметка** (2026-07-10): ✅ снята тем же днём —
  трек доведён до конца (см. закрытый маркер выше), ветка `fixed-array-value` готова к слиянию.

- **[M-vec-new-cap-chain-method-generic-erase]** (2026-07-10, P2, найден при
  lint-sanitation/spec_tests-починке `W_RETIRED_NAME` `with_capacity`) —
  чекер эрейзит `[]U.new().cap(n)` в bare `Vec` (E7301 «cannot assign value of
  type `Vec` to ... declared as `[]U`»), когда `U` — МЕТОД-уровневый (не
  receiver-уровневый и не конкретный) type-param. Минимальный репро:
  ```nova
  fn[T] []T @m[U](f fn(T) -> U) -> []U {
      mut out []U = []U.new().cap(@len())   // E7301 здесь
      for x in @ { out.push(f(x)) }
      out
  }
  ```
  `[]U.new()` ОТДЕЛЬНО резолвится верно (`Vec[U]`); `.cap(n)` (D117 `mut @cap(n)
  -> @`) отдельным statement'ом на уже-типизированной `out` — тоже верно.
  Обходной путь (используется в `spec_tests/conformance/
  d145_fn_prefix_receiver_generic.nv::d145_map`, НЕ hack — оба варианта
  одинаково каноничны): разбить на два statement'а (`mut out []U = []U.new()`
  \ `out.cap(@len())`) вместо цепочки в одном выражении. Похоже на класс
  ранее закрытого [M-http-props-mut-chain-stmt-value-copy-loss] (беглая
  `-> @`-цепочка теряет тип/идентичность на value-типе), но ТА починка была
  про chain-norm root-temp hoist для receiver уже известного типа; здесь
  корень цепочки — САМ КОНСТРУКТОР (`[]U.new()`), типизированный
  method-level generic'ом — вероятно другой путь в чекере (`assignable`/
  `resolved_cat_of`, types/mod.rs). Не расследовано глубже (вне периметра
  lint-sanitation волны) — чинить отдельным заходом compiler-codegen.

- **[M-result-ok-unit-inference-mismatch]** ✅ **CLOSED 2026-07-16** (ветка
  `fix-result-ok-unit`, sonnet) — найден 2026-07-10 при lint-sanitation/починке
  `W_RESULT_DISCARDED` в std/tls/stream.nv: `.ok()` на `Result[(), E]`
  (unit-ok тип) давал checker/codegen рассинхрон — чекер типизировал биндинг
  как `Option[E]` (Option ОШИБКИ), а codegen корректно эмитил вызов
  `Result_method_ok_nova_unit_<E>`, возвращающий `NovaOpt_nova_unit`
  (Option[()]) → CC-FAIL «initializing `NovaOpt_<E>_p` with an expression of
  incompatible type `NovaOpt_nova_unit`». **Корень — НЕ в generic-subst
  `.ok()`-резолюции** (та подставляла T/E корректно), а в
  `resolved_to_typeref` (`compiler-codegen/src/types/mod.rs`): базовый case
  `R::Unit => return None` (безобидный на ВЕРХНЕМ уровне) вызывается и
  ПОЗИЦИОННО внутри `R::Named`-арма через `.filter_map(...)` для перестройки
  generics-листа (`Result[T, E]` → `args: [T, E]`) — `filter_map` ТИХО РОНЯЛ
  `R::Unit`-arg, схлопывая `Result[(), E]` (`args: [Unit, E]`) в
  ОДНОЭЛЕМЕНТНЫЙ `Result[E]`, сдвигая E в T-слот; даунстрим subst
  (`build_recv_subst`) биндил `T→E` через arity-mismatch-fallback (позиционный
  zip) → `.ok()` типизировался как `Option[E]`. Фикс: `()` ПРЕДСТАВИМ
  (`TypeRef::Unit`) — round-trip'им вместо дропа (`R::Unit =>
  TypeRef::Unit(span)`, отдельный арм вместо catch-all `return None`).
  RED: изолированный repro (`Result[(), MyErr].ok()` в биндинге) —
  дословно тот же CC-FAIL, что в диагнозе. GREEN после фикса (compile +
  runtime). Regression-guard: `.ok()` на НЕ-unit ok-типе (`Result[[]u8, E]`)
  остался корректен. Тест: `spec_tests/conformance/result_ok_unit.nv`
  (unit-ok pinning ×2 + non-unit регресс-защита ×2; PASS в изолированном
  прогоне до слияния в общий пир-модуль, per test-conventions workflow).
  Обход в nova-tls (`src/stream.nv`, отдельный репозиторий) снят ×2 — вернул
  `.ok()`-форму на ветке `fix-result-ok-unit` (репо nova-tls), проверено
  `nova check src/stream.nv` против пропатченного nova: байт-идентичный
  список pre-existing `W_CONSUME_KEYWORD_UNNECESSARY`-ошибок (несвязанный
  D180-долг, чинится параллельно на ветке `fix-d180-consume-tests`) до/после
  правки — ноль новых диагностик.

## [M-strict-effects-conformance-sweep] — `--strict-effects` debt snapshot: std/ + examples/ ЧИСТЫ (2026-07-13, Plan 197)

Plan 197 добавил экспериментальный флаг `--strict-effects` (`nova check/build/test
--strict-effects`, off by default — nova-cli/src/main.rs + compiler-codegen/src/
strict_effects.rs) с двумя opt-in диагностиками поверх обычного чекера:
`E_UNDECLARED_TRANSITIVE_EFFECT` (D62 §Правило 1 — транзитивный эффект без
объявления/lexical with-хендлера, сейчас silent, под флагом — hard error) и
`E_EFFECT_ERASED_IN_FN_TYPE` (присвоение/передача/return fn-значения в более
узкий по эффектам fn-тип — erasure). Реализация: types/mod.rs::CapabilityCtx::
check_transitive_effect_strict (диагностика 1, переиспользует существующий
with_handler_stack/declared_effects walk) + strict_effects.rs::check_effect_erasure
(диагностика 2, отдельный syntactic pass). См. spec/decisions/04-effects.md D62,
spec/open-questions.md.

**Снапшот долга (2026-07-13, commit на ветке `strict-effects`, worktree nova-197):**
`nova --strict-effects check std --format short` → `PASS: 126 FAIL: 21 WARN: 250`
— **байт-в-байт идентично** прогону БЕЗ флага (diff отсортированных выводов
пустой); все 21 FAIL — pre-existing негативные фикстуры (`*_neg.nv`/`neg/`:
serde/consume/D131 и т.п.), НЕ связаны с эффектами. `nova --strict-effects check
examples --format short` → `PASS: 30 FAIL: 0 WARN: 51` (`examples/_wip/` вне
скана — pre-existing tooling-skip, не Plan-197-specific). **Итог: ZERO строк
нарушений** в обоих деревьях — `std/` и `examples/` уже конформны
`--strict-effects` без единой правки.

Машинно-парсимый список (`docs/plans/wip/strict-effects-debt.txt`,
`путь:строка:вид:недостающий-эффект`) создан пустым (0 data-строк, только
header-комментарий с датой/командой) — **следующему haiku-агенту делать
нечего**: миграция аннотаций std/examples не требуется, флаг можно включать
в конвенцию сборки std/examples уже сейчас (владелец: «внутренние модули std
и программы ДОЛЖНЫ собираться с `--strict-effects`» — это условие уже
выполнено). Если объём кода в std/examples вырастет и появятся нарушения —
перезапустить `nova --strict-effects check std examples --format short` и
пополнить `strict-effects-debt.txt` по тому же формату.

## [M-173-trace-not-in-child-error] — трасса/throw-site не протягиваются через эскалацию scope (P3, 2026-07-13)

Вскрыто волной trace-per-fiber (чекпоинт волны удалён при закрытии, см. git-историю; §201.4): `child_error[]` несёт
msg/kind/payload/tid, но НЕ throw-site и НЕ propagation-trace ребёнка → при эскалации
через `nova_rethrow_scope` наружу uncaught-abort печатает трассу РОДИТЕЛЯ (пустую/чужую),
дамп ребёнка теряется. Диагностика only (catch-механика корректна). Фикс-направление:
поля site+trace-снапшот в NovaChildError при фиксации детской ошибки, rethrow
восстанавливает. Тест-блокер: наблюдать трассу с Nova-уровня пока нечем — нужен
accessor (`error_trace() -> []str`?) либо проверка stderr-дампа раннером.
| `[M-div-neg-overflow-trap]` | Найдено при ревью плана 206 (2026-07-15). `Div`/`Mod` лоуэрятся в СЫРЫЕ C `/`/`%` БЕЗ guard (emit_c ~8204/~28047) → **деление на ноль = UB (x86 #DE → SIGFPE, неконтролируемый крэш, не паника Nova)**; `INT_MIN / -1` тоже UB. `neg(INT_MIN)` — overflow-UB аналогично. Это отдельно от `__builtin_*_overflow`-примитива (у div/neg своего builtin нет). Нужно: (1) div/mod — **всегда трапить деление-на-ноль** чистой паникой (крэш-вектор, приоритет!) + trap `INT_MIN/-1`; (2) `neg` — trap `-INT_MIN`; (3) методы `@checked_div`/`@checked_neg`/`@wrapping_neg`. Вне рамок 206 (иной механизм), но safety-история неполна без этого. div-by-zero = **P1** (частый крэш). **Оформлен подпланом [206.1](206.1-div-neg-trap.md).** | Plan 206.1 | P1 |

## [M-tls-xpkg-tlsversion-value-ptr-dispatch] — cross-package sum-type method value/pointer-ABI mismatch (P1, 2026-07-15) — ✅ РЕШЕНО

**РЕШЕНО 2026-07-15 (opus, verified):** корень — НЕ в receiver-ABI-модели sum-type, а в
одной точке type-инференса `??`. Legacy-ветка `ExprKind::Coalesce` в
`infer_expr_c_type` (`emit_c.rs` ~54063) стрипила `NovaOpt_`-префикс и возвращала
payload-идентификатор `Nova_TlsVersion_p` **ВЕРБАТИМ**, не разворачивая
sanitized-pointer-маркер `_p` обратно в `Nova_TlsVersion*` (в отличие от
Coalesce-ЭМИССИИ ~30988, которая уже звала `desanitize_c_from_ident`). Битый тип
`??`-локала (`Nova_TlsVersion_p version`) отравлял ВНИЗ по потоку receiver-мэнглинг
метод-диспатча → on-demand эмиссия метода `Nova_Nova_TlsVersion_p_method_to_str` с
несуществующим типом `Nova_TlsVersion_p*` → CC-FAIL. Локальные однотипные sum-type
НЕ были задеты — они резолвятся через Channel-2 (чекерский resolved-type →
чистый `Nova_Ver*`); cross-package падал в эту legacy-ветку (канал промахивался).
**Фикс:** развернуть маркер через `Self::desanitize_c_from_ident(sani)` (идемпотентно
для value-payload `nova_int`/`nova_str`/`NovaValue_…` → byte-identical). Runtime/codegen-
фикс, НЕ язык-меняющий → D-амендмент не нужен. Verified: `echo_client.nv` **линкуется**
(был C-error), `echo_server.nv` не регрессировал (linked), локальный `Option[enum] ??
default` + метод собирается. Замечание (вне периметра, не воспроизведено): соседняя
`Try/Bang`-ветка (~54152) имеет тот же нераскрытый `_p` для cross-package `Option[Sum]?`
— другой символ/путь, оставлен как наблюдение.

**UPDATE 2026-07-15 (интегратор, verified):** Симптом 1 (decode_utf8) был **устаревшим тегом nova-tls** (lock пинил `510acc25` = v0.1.0, до-196.7) — **СНЯТ** пере-тегом nova-tls **v0.1.1** (`79440c53`, с 196.7 `to_str`) + пере-резолв lock; `echo_server.nv` + weather-live aggregator теперь **линкуются** (verified). Симптом 2 (TlsVersion) — **РЕАЛЬНЫЙ codegen-баг, персистит с v0.1.1**: `echo_client.nv` → `error: passing 'Nova_TlsVersion' (value) to parameter 'Nova_TlsVersion_p*'` — codegen эмитит `Nova_Nova_TlsVersion_p_method_to_str(version)` (передаёт ЗНАЧЕНИЕ), а метод объявлен `(Nova_TlsVersion_p* nova_self)` (ждёт УКАЗАТЕЛЬ). Cross-package sum-type-метод (`TlsVersion.@to_str()`, объявлен nova-tls `config.nv:41`), вызванный из потребителя (entry-пакет echo_client), генерит value-vs-pointer ABI-mismatch на receiver. Блокирует `echo_client.nv` LINK (echo_server/aggregator НЕ задеты — не зовут TlsVersion.@to_str). Класс — родственен D39 (cross-package/mono method-receiver). Зона: `compiler-codegen/src/codegen/emit_c.rs` (receiver value/ptr для sum-type-метода внешнего пакета). Старое: cross-package decode_utf8/tlsversion dispatch — decode-часть была stale-tag, оставлена ниже как контекст.

### (исторический контекст — Симптом 1 был устаревшим тегом, снят; Симптом 2 актуален)

Вскрыто НЕ в vendor-FFI-коде (тот фикс отдельно верифицирован рабочим — см. соотв. запись выше/в simplifications.md), а как побочный блокер при попытке довести `examples/tls/echo_server.nv`/`echo_client.nv` до полного LINK на чистом чекауте (без вручную скопированных mbedTLS-либ). Подтверждено С `NOVA_CACHE=0` (сброс content-addressed build-кэша, `nova-cli/src/main.rs::build_cache`, — так что это НЕ артефакт устаревшего кэша) на текущем main + слитой `fix-consume-cleanup` (70c4eff02).

- **Симптом 1 (`echo_server.nv`, `echo_client.nv`):** резолвленный (запиненный в `nova.lock`, коммит `510acc25…`) `nova-tls`'s `src/stream.nv:57` зовёт `msg_bytes.decode_utf8()` — СВОЙ extension-метод на `[]u8`, заведённый как обход другого известного маркера `[M-174.1-to-str-name-collision-codegen-bug]` (комментарий в stream.nv:239 — `@to_str()` коллидирует с `TlsError`'s собственным `@to_str()`). Генерируемый C: `msg_bytes->decode_utf8() /*?? unsupported */` → `error: no member named 'decode_utf8' in struct Nova_Vec____nova_byte` (codegen не диспатчит метод в функцию, а наивно эмитит member-access — `ExprKind::Coalesce`'s generic fallback, emit_c.rs ~31012-31014 — сигнал, что `infer_expr_c_type`/`emit_expr` для ЛЕВОЙ части `??` не нашли метод `decode_utf8` в method-таблицах для КРОСС-ПАКЕТНОГО extension-метода).
- **Симптом 2 (`echo_client.nv` только):** `Nova_TlsVersion_p` используется как ИМЯ ТИПА (`unknown type name 'Nova_TlsVersion_p'`) там, где должен быть указатель `Nova_TlsVersion*` — плюс value/pointer mismatch на инициализации (`TlsVersion_p version = (...)`) и передаче в `@to_str()`. `TlsVersion` — sum-type (`enum`), объявленный в nova-tls (`src/config.nv:35`), с собственным `@to_str()` (`config.nv:41`). Тот же класс — cross-package метод/тип на СВОЁМ sum-type, вызываемый ИЗ ПОТРЕБИТЕЛЯ (entry-пакет).
- **Гипотеза (не подтверждена глубже — вне периметра этой волны):** оба симптома — один класс: codegen method-dispatch/mono-таблицы для extension-методов/sum-type-методов, ОБЪЯВЛЕННЫХ во ВНЕШНЕМ (git/path) пакете, не полностью прописываются при вызове ИЗ ДРУГОГО пакета (entry-приложение) — вероятно, эти конкретные examples (`tls/echo_server.nv`/`echo_client.nv`) ПЕРВЫЙ раз реально прогоняются end-to-end через ПОЛНОСТЬЮ чистую (`NOVA_CACHE=0`, без вручную положенных mbedTLS-либ) сборку — предыдущие проверки (включая консью-cleanup волну, см. запись «187 cross-package consume-cleanup DCE-дыра ЗАКРЫТА», заявлявшую `built` 2.28МБ) могли неявно переиспользовать content-addressed build-кэш (`build_cache::load_c`, кэш пишется ПОСЛЕ codegen, ДО реальной C-компиляции — т.е. может закэшировать уже битый `.c`, если бы он там был; но, что важнее, могли просто НЕ делать `NOVA_CACHE=0`-чистый прогон) — **рекомендация владельцу: перепроверить ту верификацию с `NOVA_CACHE=0`, прежде чем доверять «built» как доказательству того, что этот путь рабочий**.
- **Не путать** с `[M-176-collision-variant-method-dispatch]` (тот — про D381-коллизию имени суммы ВНУТРИ одного CU; здесь тип не коллидирует, проблема именно в ПАКЕТНОЙ границе).
- Блокирует полный `nova build`/`test` LINK для `examples/tls/echo_server.nv` и `echo_client.nv` (компиляция валится на C-compile stage, ДО линковки — vendor-FFI автосборка mbedTLS отрабатывает корректно И ДО этой ошибки). НЕ блокирует vendor-FFI-фикс (verified независимо — сообщение `nova: vendor FFI lib(s) […] built` + реальные `.lib`-архивы в `native/lib`). Не исследовано глубже (чинить отдельным заходом compiler-codegen/checker method-dispatch, вне периметра vendor-ffi-волны).
| `[M-consume-block-cancelerror-bare-cu]` | **✅ РЕШЕНО 2026-07-15 (sonnet, worktree `nova-ccancel`, ветка `fix-consume-cancelerror-bare`, не запушено).** Уточнённый корень (НЕ tree-shaking по usage): `ScopeOutcome` (нужен для `@cleanup`-сигнатуры) живёт в `std/prelude/core.nv`, `CancelError` — в ОТДЕЛЬНОМ `std/prelude/errors.nv` (Plan 62.F splittable prelude); `#prelude(core, ...)` без `errors` НИКОГДА не мёржит `.nv`-декларацию `CancelError`, а `assign_scope_outcome_from_frame` (emit_c.rs) безусловно эмитит `Nova_CancelError` на КАЖДОМ FAIL/INTERRUPT run-site ЛЮБОГО consume-cleanup — **и escape-, и statement-форма (d188) ОБЕ репродуцируют** (уточнение к исходной заявке: разница была не escape/statement, а partial-prelude/effect-context). Fix (Path B, эмпирически — Path A форс-инжекта `errors`-прелюда тянет ПРЕДСУЩЕСТВУЮЩИЙ независимый баг, см. `[M-prelude-errors-startswith-not-selfcontained]` ниже): `CancelError` добавлен в `RUNTIME_DEFINED_TYPES` (emit_c.rs) + hand-written `Nova_CancelError{reason:nova_str}` в `nova_rt/array.h` (паттерн `Error`/`RuntimeError`) — typedef всегда доступен независимо от prelude-подмножества; `emit_type_decl` уже skip'ает `.nv`-эмиссию для RUNTIME_DEFINED_TYPES-имён → no redefinition когда `.nv`-декларация ТОЖЕ смёржена (default full prelude); `err is CancelError` narrowing/type-id — не тронуты (были независимы от `.nv`-мёржа уже до фикса). Гейт: минимальный escape-repro + statement-repro (`#prelude(core,runtime)`) зелены; full-prelude сценарий с explicit `err is CancelError` narrowing — компилится без redefinition; `nova test std/src/net/tcp_share_test.nv` PASS; `nova test std/src/net` (весь модуль) PASS. spec_tests/conformance НЕ гонялся (мега-CU, CPU-дисциплина волны) — точечная замена репро-фикстурами. Детали: чекпоинт волны удалён при закрытии, см. git-историю. | floating (codegen/consume-block) | **✅ РЕШЕНО** |
| `[M-prelude-errors-startswith-not-selfcontained]` | **НОВЫЙ (найден 2026-07-15, sonnet, побочный эффект разведки Path A выше — не путать с ним).** `std/prelude/errors.nv` заявляет «ZERO imports / self-contained на primitives» (шапка файла), но `MultiError.@find_first_panic()` (строки ~241-251) использует `str.starts_with(...)` — метод, объявленный в `std/runtime/string/search.nv`, который мёржится в compile-unit ТОЛЬКО через полную default-facade (`std/prelude.nv`'s `import std.runtime.string.{...}`), НЕ через `#prelude(errors)` / `#prelude(core, runtime, errors)` (Plan 62.F партиал-прелюдия). Repro (без consume/cleanup вообще): `#prelude(core, runtime, errors)` + `RuntimeError.DivByZero` где-либо → **CC typecheck FAIL** `[E7320]/[E_UNKNOWN_METHOD] no method starts_with on primitive type str` (тип-чек проверяет ВСЕ смёрженные декларации структурно, независимо от reachability). Означает: `#prelude(..., errors)` партиал-подмножество СЕЙЧАС полностью нерабочее в любом виде, где `errors.nv` мёржится не через full default facade. Fix (не сделан, вне периметра [M-consume-block-cancelerror-bare-cu]): либо `.starts_with`/`.ends_with` — checker built-in примитив (мирроря другие core str-методы), либо явный forced-merge `std.runtime.string.search` вместе с `errors`-сабмодулем в partial-prelude резолве. | floating (prelude/Plan62.F) | **P3 — не запланирован** |
| `[M-concurrency-retry-test-cc-fail]` | Найдено SSE-hang агентом (2026-07-15, sonnet) при точечной регрессии std/concurrency: `std/src/runtime/retry_test.nv` — **CC-FAIL** в изоляции (падает и БЕЗ правок SSE-фикса — тот трогал только runtime.c/fibers.h; supervised_deadline/supervisor/rate_limiter — PASS). Предсуществующий, не связан с SSE. **Нужна перепровка интегратором** (targeted `nova test std/src/runtime` после текущего гейта) → подтвердить репро + локализовать (какой C-symbol/тип undefined) → отдельный codegen/std-заход. | floating (concurrency/retry) | **P2 — нужна перепроверка** |
| `[M-tls-tests-consume-keyword-d180-drift]` | Найдено П9-агентом (2026-07-16, sonnet): 12 сайтов `consume conn = accepted!!` в тестах nova-tls (cert_modes_test.nv×6, mtls_test.nv×4, handshake_test.nv×2) триггерят `[W_CONSUME_KEYWORD_UNNECESSARY]` (D180) под текущим компилятором → CODEGEN-FAIL ВСЕГО tls-тест-модуля (один CU). Дрейф компилятор↔внешняя репа: тесты писались до D180-линта. Фикс = миграция тестов nova-tls (убрать лишний `consume` по D180) + прогнать модульные тесты; тесты НЕ ослаблять (это не ослабление — приведение к действующей норме D180). | nova-tls / tests | **P2** |
| `[M-185-lint-deny-gate]` | **Найдено аудитом планов ≥150 (2026-07-16).** Plan 185 (статус файла: ✅ «ПОЛНАЯ ВЕРСИЯ» 2026-07-09) обещает `nova lint --deny` (W→E promotion) как гейт для CI/агентской поставки (dev-workflow: «агентская поставка обязана прогонять `nova lint --deny` по правленным файлам»; `nova lint --deny std/` заявлен чистым). Фактически `nova lint`-подкоманда (`nova-cli/src/main.rs` `Cmd::Lint`) поддерживает только `--rule`/`--list-rules`/`--include-runtime`/`--skip`/`--quiet` — флага `--deny` НЕТ вообще (grep по `nova-cli/src/` = 0). Обещанный CI/приёмочный гейт не существует. | Plan 185 / nova-cli | P2 |
| `[M-159.1-onexit-drop-overprune]` | **Найдено аудитом планов ≥150 (2026-07-16).** Plan 159.1 (Ф.1, method-reachability DCE) ещё не начат, но риск уже виден по текущей модели: `collect_used_names` (DCE reachability-collector) не имеет seed'а для методов, достижимых ТОЛЬКО через scope-exit/`on_exit`/`@cleanup`-drop (a не через прямой синтаксический call-сайт) — DCE может тихо срезать такой метод как «недостижимый», хотя рантайм зовёт его на выходе из scope. Риск тихой порчи (метод существует, но при определённой комбинации DCE его тело не эмитится). Требует seed-расширения reachability-коллектора ДО или В РАМКАХ Ф.1 159.1. | Plan 159.1 Ф.1 / codegen DCE | **P1** |
| ~~`[M-198-f4c-compiler-findings]`~~ | **✅ РАЗОБРАН 2026-07-17 (Plan 212 пункт 7, sonnet, worktree `nova-198rv`, бинарь пере-собран на коммите `696d834b4`).** Зонтичный маркер пере-проверен по всем 9 находкам актуальным компилятором (изолированные fixture-репро per-item, полный `spec_tests/conformance` НЕ гонялся — инструкция волны). Итог: **(1) и (2) — живые, воспроизводятся** → вынесены в отдельные `[M-198-f4c-1-privfile-type-not-discriminated]` / `[M-198-f4c-2-local-not-shadow-crossfile-topfn]` (ниже). **(3) alias-import folder-peer и (4) handler-литерал match-arm capture — НЕ воспроизводятся** ни в изоляции (solo-модуль), ни как genuine folder-module peer (2 файла); оригинальные victim-фикстуры (`f1_alias_call_pos`, `f2_whole_module_pos`, `f3_typed_result_err`) уже проходили на полном merged CU при финальном FIN-6 тэлли 2026-07-13 (`PASS 501/FAIL 4`, эти файлы НЕ среди 4 известных FAIL) — комбинация исторического full-scale PASS + свежего изолированного PASS достаточна для закрытия без повторного полного прогона; если всплывёт заново на полном гейте — переоткрыть с свежим репро. **(5) std-internal classify-капча — НЕ переверено на заявленном масштабе** (полный corpus запрещён этой волной; изолированный репро с корректным `import std.net.{NetError}` — чист; оригинальный триггер-файл не идентифицирован) → вынесено как `[M-198-f4c-5-std-internal-symbol-capture]` (ниже), P3, статус неопределён. **(6) bench.\* ICE в test-блоках и (7) extern "nova" tuple-return CC-FAIL — живые, воспроизводятся** (существующие quarantine-фикстуры `fixtures/ice_blocked/p2_bench_namespace_callable.nv` / `spec_tests/fixtures/known_red/t4_sqlite_e2e_ok.nv` подтверждены на актуальном бинаре) → вынесены в `[M-198-f4c-6-bench-intrinsic-test-block-ice]` / `[M-198-f4c-7-extern-nova-tuple-return-ccfail]` (ниже). **(8) priv(file)-fn bleed — ЗАКРЫТО фиксом `7542e0013`** (2026-07-14, D307 §5.3 facet-B, ДО этой волны) — переподтверждено изолированным репро на актуальном бинаре, PASS. **(9) file-scoped `#unchecked` — MOOT/ЗАКРЫТО**: `#unchecked` полностью ретрактирован (Plan 194) — `#unchecked { … }` теперь `error: unexpected '#' in expression`, конструкция физически не существует, репро невозможно И не нужно. Полная таблица вердиктов — `docs/simplifications.md` (запись 2026-07-17, Plan 212 п.7). | Plan 198 Ф.4c → расщеплён | ✅ DONE |
| ~~`[M-198-f4c-1-privfile-type-not-discriminated]`~~ | **✅ ЗАКРЫТО 2026-07-17 (sonnet, worktree `nova-privtype`, бинарь на коммите поверх `99f0021f9`).** Корень: `compiler-codegen/src/types/mod.rs` `TypeCheckCtx.types: HashMap<String, &TypeDecl>` — имя-only ключ, `TypeCheckCtx::build`'s регистрационный цикл (`types.insert(td.name.clone(), td)`) молча перезаписывал слот при co-presence двух `priv(file) type Rect` разных peer-файлов ОДНОГО folder-module (последний в `module.items` побеждал) — `f3_check_member_ctx` (field-access) и `infer_expr_type`'s Member-ветка (field-type inference) читали этот ЕДИНЫЙ слот без файлового контекста. Зеркало 2d5f64e91 (D307 fn-резолв: `sig.fn_decls: Vec<&FnDecl>` + caller-file фильтр в `f1_check_call`) — но для типов Vec-реестра не было, пришлось завести parallel lossless side-table `file_local_types: HashMap<FileId, HashMap<String,&TypeDecl>>` (тот же паттерн, что уже используется для `sum_variant_names` — комментарий в файле про co-presence одноимённых sum-типов) + хелпер `types_get_for_file`; wired в оба checker-сайта. Once checker перестал ошибаться, всплыли ЕЩЁ 3 симметричных codegen-сайта (emit_c.rs) с ТОЙ ЖЕ болезнью (name-only, не file-aware): (1) struct/tag naming `Nova_<Name>` — оба peer-файла эмиттили ИДЕНТИЧНЫЙ C-struct → "redefinition"; новая `file_priv_type_c_names: HashMap<(FileId,String),String>` (зеркало `private_const_c_names` для `priv(file) const`) + `def_type_base`/`ref_type_base` теперь читают её ПЕРВОЙ, до D381 cross-module `colliding_type_names`; (2) 5 `current_emit_file_id`-гейтов были условны ТОЛЬКО на D381 cross-module коллизию (`!colliding_type_names.is_empty()`) — расширены новым хелпером `any_type_file_collision()` (D381 ИЛИ same-module per-file коллизия), иначе `ref_type_base` не имел файлового контекста для резолва (forward-decl эмиттил голый `Nova_Rect` → "unknown type"); (3) `emit_record_lit` для БАРЕ (1-сегментного) record-литерала (`Rect { w, h }`) вообще не звал `ref_type_base` (D381 покрывал только явную 2-сегментную `Type.Variant{…}` форму) — оба peer-файла падали в "unknown type" null-stub → runtime NULL-деref crash. Все 4 сайта пофикшены за одну волну (§4а zero-tolerance). Фикстура (`a.nv`/`b.nv`) перенесена `spec_tests/fixtures/known_red/privtype_file_discrimination/` → `spec_tests/conformance/privtype_file_discrimination/` (module `conformance.privtype_file_discrimination`, D78 rev-3 parent.X), GREEN (PASS 1/FAIL 0, оба test-блока). `known_red/README.md` — запись убрана. δ0: `nova check` по 15 std-подпапкам (runtime/collections/encoding/data/identifiers/math/text/net/path/time/concurrency/os/fs/unicode/checksums) — 112 PASS, 15 FAIL — все 15 суть намеренные `*_neg.nv`/`EXPECT_COMPILE_ERROR` фикстуры (не регрессия), включая пины `runtime/char.nv`+`char_test.nv`+`sync.nv`+`sync_test.nv` — чисто. Побочная находка (НЕ фиксилась, вне периметра): `b.nv`'s исходный `r.tag.into_str()` (int-поле) ловил **отдельный, пред-существующий** P67-LEGACY ICE (`emit_c.rs` method-call return-type-unknown на chained field+primitive-method) — репро подтверждено СТАНДАЛОНЕ (без priv(file), без коллизии) на ОБОИХ бинарях (main repo pre-existing + этот воркtree) — вынесено как `[M-198-f4c-1-into_str-primitive-chain-p67]` ниже; фикстура переписана на прямое сравнение полей (`describe(r) == "box"` + `assert(r.tag == 7)`), не теряя тестового намерения (обе СВОИ формы `Rect` читаются корректно). | Plan 198 Ф.4c / checker+codegen (D307 типы) | ✅ DONE |
| `[M-198-f4c-1-into_str-primitive-chain-p67]` | **NEW 2026-07-17 (найдено побочно при закрытии M-198-f4c-1, sonnet).** `<primitive-field>.into_str()`/`.to_str()` на ЧИСТО ЦЕПОЧЕЧНОМ receiver'е (`r.tag.into_str()`, где `tag int` — прямой field-access, БЕЗ промежуточной ro-переменной) триггерит `internal error … [P67-LEGACY] method call `.into_str`/`.to_str` return type unknown — checker must annotate` — ЧЕКЕР пропускает вызов (метод не существует на `int` вообще — прямой `x.to_str()` на локальной `int`-переменной корректно даёт `[E_UNKNOWN_METHOD]`), но permissive-путь для НЕ-резолвящегося receiver-типа Member-цепочки не ловит это ДО codegen → P67 ICE вместо диагностики. Живой репро СТАНДАЛОНЕ (без priv(file)/folder-module), подтверждён на main repo текущем бинаре И на этом воркtree — НЕ связан с priv(file)-дискриминацией, НЕ являлся причиной M-198-f4c-1's runtime-фейла (тот был отдельным, уже пофикшенным record-lit/struct-naming багом). Репро: `type Rect { name str, tag int }`; `fn describe(r Rect) -> str => r.name + ":" + r.tag.into_str()` (или `.to_str()`) → `internal error at compiler-codegen/src/codegen/emit_c.rs:~52349`. Workaround: избегать чейнинга — привязать поле к `ro`-переменной ПЕРЕД вызовом (хотя даже это на `.to_str()` корректно даёт `E_UNKNOWN_METHOD`, т.к. `int` такого метода не имеет — идиома для int→str не `.to_str()`/`.into_str()`, а строковая интерполяция `"${expr}"`). Не расследовано глубже (вне периметра волны) — нужен отдельный трек: (а) чекер обязан либо резолвить возврат ЛЮБОГО метода, найденного через Member-цепочку, либо явно отклонять несуществующий метод (симметрично прямому non-chained пути); (б) альтернативно — codegen P67-гейт обязан давать нормальную диагностику, не ICE. | обнаружено при 198-f4c-1 / checker+codegen (Member-chain method resolve) | P2 |
| `[M-198-f4c-2-local-not-shadow-crossfile-topfn]` | **OPEN 2026-07-17 (Plan 212 пункт 7, sonnet, живой репро на бинаре `696d834b4`).** Локальная переменная (`ro f = helper`) НЕ затеняет top-level `fn` того же имени, объявленный в ДРУГОМ файле того же folder-module — call-site биндится к чужому top-level `fn` вместо локала. Репро: `a.nv` объявляет `fn f(y str) -> str`; `b.nv` (`ro f = helper`, `helper int->int`) зовёт `f(21)` → `[E7301] cannot pass int as argument y of type str` / `note: parameter y declared here --> a.nv`. Фикстура: `spec_tests/fixtures/known_red/local_shadows_topfn/{a,b}.nv`. Родственно, но НЕ дубликат уже закрытого `[M-168-resize-with-free-fn-shadow]` (тот фикс — closure-ПАРАМЕТР vs module-level fn внутри Vec.resize_with/fill_with; этот — ЛОКАЛ vs cross-file top-level fn, отдельный незакрытый путь резолва). | Plan 198 Ф.4c / checker (name resolution) | **P2** |
| `[M-198-f4c-5-std-internal-symbol-capture]` | **НЕ ПЕРЕВЕРЕНО на заявленном масштабе, статус неопределён (Plan 212 пункт 7, sonnet, 2026-07-17).** Исходная заявка (198-redo-notes.md, найдено при оригинальной ~1000-файловой merged-CU миграции): `std.net`'s внутренний (неэкспортируемый) `fn classify(msg str) -> NetError` в merged CU эмитился как вызов ПОЛЬЗОВАТЕЛЬСКОГО `nova_fn_10spec_tests11conformance8classify` (mangling-коллизия по голому имени, potentially soundness-grade capture). Оригинальный триггер-файл (переименован в обход `repro_classify`, коммит `559d52880`) не идентифицирован/не сохранён как отдельная регрессия — grep по актуальному дереву не находит коллидирующего пользовательского `classify` на корне `spec_tests.conformance`. Изолированная репро-попытка 2026-07-17 (user `fn classify(x int)` + `import std.net.{NetError}` + `NetError.from_code(...)`, реально зовущий std-internal classify, в МАЛОМ non-root модуле) — PASS, коллизия не воспроизвелась. Полный `spec_tests/conformance` (~1000 файлов, где исходно нашли баг) НЕ прогонялся (запрещено инструкцией волны 212.7). Ни закрыт, ни живой репро не найден — нужен отдельный заход с бюджетом на полный corpus для окончательного вердикта. | Plan 198 Ф.4c / codegen mangling | P3 |
| `[M-198-f4c-6-bench-intrinsic-test-block-ice]` | **OPEN 2026-07-17, живой ICE подтверждён (Plan 212 пункт 7, sonnet, бинарь `696d834b4`).** `bench.*`-интринзики (`.opaque`, `.now_ns`, ...), использованные ВНУТРИ `test { }` блока (а не `bench { measure { } }`), роняют компилятор ЦЕЛЫМ процессом: `internal error at emit_c.rs:52127: [P67-LEGACY] method call .opaque return type unknown — checker must annotate`. Фикстура уже существует и квалифицирована: `spec_tests/conformance/fixtures/ice_blocked/p2_bench_namespace_callable.nv` (walker-skip). Строка ICE сдвинулась (48774 → 52127) из-за дрейфа кода за эти дни — тот же класс `[P67-LEGACY]`, не новая регрессия. | Plan 198 Ф.4c / codegen (P67-LEGACY) | **P2** |
| `[M-198-f4c-7-extern-nova-tuple-return-ccfail]` | **OPEN 2026-07-17, живой pre-existing баг подтверждён (Plan 212 пункт 7, sonnet).** `extern "nova" fn` с tuple-return (`-> (*(), int)`) не линкуется «из коробки» — Plan 115 D214 FFI-механизм задокументирован как незавершённый в исходном коде (`examples/ffi/sqlite_mini.nv`/`sqlite_mini_ffi.h` цитируют followup'ы `[M-115-ffi-build-pipeline]`/`[M-115-examples-ffi-real-build]`, но ни один из них НИКОГДА не заводился как backlog-маркер — это первая запись долга здесь). Репро 2026-07-17: `spec_tests/fixtures/known_red/t4_sqlite_e2e_ok.nv` + временная копия `sqlite_mini_ffi.h` в `nova_rt/` (не коммичено, только для теста) → `lld-link: undefined symbol: nova_fn_mini_sqlite_open` (extern-декларация ждёт C-символ с `nova_fn_`-манглингом; embedded mini-shim предоставляет голые C-имена без префикса — `--c-shim` CLI-инфраструктура из Plan 115 followup так и не построена). Фикстура уже в дереве (`spec_tests/fixtures/known_red/t4_sqlite_e2e_ok.nv`, README уже документирует известный-красный статус). | Plan 115 (D214 FFI) / nova-cli --c-shim | P3 |
| `[M-169.2-red-audit-orphaned]` | **Найдено аудитом планов ≥150 (2026-07-16), зонтичный.** `docs/plans/169.2-red-audit.md` (2026-06-20) именует 10 компилятор-маркеров как «Handoff Plan 172 агенту»; проверка показала, что **6 из них никогда не попали ни в `backlog-followups.md`, ни в `simplifications.md`** (не 8 — сверено grep'ом, поправка к первичной оценке): `[M-170.1-priv-file-types-methods]` (priv(file) не дискриминирует type-struct/method-символы), `[M-169.2-consume-param-not-counted]` (передача во consume-параметр не считается consumption → ложный D133-not-consumed), `[M-169.2-sum-variant-ctor-mono]` (generic-sum unit-variant ctor не мономорфизируется → undefined symbol), `[M-169.2-default-body-protocol-dispatch]` (codegen E7320 на protocol-методе с default body), `[M-169.2-consume-method-resolve]` (E7320 на `consume @close()`-ресивере), `[M-169.2-effect-method-ret-fallback]` (return-type метода с эффектом резолвится в `int`-fallback вместо `str`). (Три других поимённых маркера — `[M-vec-shadow-leak-e7310]`, `[M-169.2-vec-fn-empty-literal-nova-int]`, `[M-91.18-import-gated-str-methods]` — уже присутствуют в backlog, орфанами не являются.) Улики-фикстуры для всех 37 «красных» folder-module этого аудита удалены волной `011aadde5` (198 Ф.2, «delete confirmed-STALE nova_tests fixtures», 1158 файлов) — статус каждого бага на АКТУАЛЬНОМ компиляторе неизвестен, нужна пере-проверка репро с нуля прежде чем чинить. | floating (169.2 / Plan 172 наследие) | P3 |
| `[M-198-f4c-compiler-findings]` | **Найдено аудитом планов ≥150 (2026-07-16), зонтичный.** `docs/plans/wip/198-redo-notes.md` §«Находки-дефекты компилятора (Ф.4c-очередь)» перечисляет 9 классов компилятор-находок, вскрытых merged-CU миграцией, ни одна из которых не имеет `[M-…]`-маркера в backlog: (1) `priv(file)`-типы не файл-дискриминируются в checker-резолве (use-site биндится к чужому одноимённому priv-типу); (2) локал/параметр не затеняет top-level fn при вызове (биндится к чужому top-level `fn` того же имени); (3) alias-import (`import X as h`) в folder-module peer — codegen эмитит `h.fn(...)` буквально (undeclared identifier); (4) handler-литерал: биндинг match-арма ошибочно считается захватом внешнего локала; (5) std-internal вызов захвачен пользовательским символом (манглинг-коллизия, potentially soundness-grade); (6) `bench.*`-интринзики внутри test-блоков = ICE (`emit_c.rs:48774`); (7) `extern "nova" fn` + tuple-return CC-FAIL (pre-existing Plan 115 FFI-регрессия); (8) priv(file)-fn bleed на `method_call_never_static`/`scalar_only_empty` (карантин в standalone/); (9) file-scoped `#unchecked` теряется в folder-module (`module_unchecked_pos`/`unchecked_invariant_pos`) — вероятно MOOT после ретракции `#unchecked` (Plan 194), требует переверки. | Plan 198 Ф.4c / checker+codegen | P2 |
| `[M-169.2-red-audit-orphaned]` | **Найдено аудитом планов ≥150 (2026-07-16), зонтичный.** `docs/plans/wip/169.2-red-audit.md` (2026-06-20) именует 10 компилятор-маркеров как «Handoff Plan 172 агенту»; проверка показала, что **6 из них никогда не попали ни в `backlog-followups.md`, ни в `simplifications.md`** (не 8 — сверено grep'ом, поправка к первичной оценке): `[M-170.1-priv-file-types-methods]` (priv(file) не дискриминирует type-struct/method-символы), `[M-169.2-consume-param-not-counted]` (передача во consume-параметр не считается consumption → ложный D133-not-consumed), `[M-169.2-sum-variant-ctor-mono]` (generic-sum unit-variant ctor не мономорфизируется → undefined symbol), `[M-169.2-default-body-protocol-dispatch]` (codegen E7320 на protocol-методе с default body), `[M-169.2-consume-method-resolve]` (E7320 на `consume @close()`-ресивере), `[M-169.2-effect-method-ret-fallback]` (return-type метода с эффектом резолвится в `int`-fallback вместо `str`). (Три других поимённых маркера — `[M-vec-shadow-leak-e7310]`, `[M-169.2-vec-fn-empty-literal-nova-int]`, `[M-91.18-import-gated-str-methods]` — уже присутствуют в backlog, орфанами не являются.) Улики-фикстуры для всех 37 «красных» folder-module этого аудита удалены волной `011aadde5` (198 Ф.2, «delete confirmed-STALE nova_tests fixtures», 1158 файлов) — статус каждого бага на АКТУАЛЬНОМ компиляторе неизвестен, нужна пере-проверка репро с нуля прежде чем чинить. | floating (169.2 / Plan 172 наследие) | P3 |
| `[M-205-f2-consumer-switch]` | **Найдено аудитом планов ≥150 (2026-07-16).** `nova-compress` (внешний пакет, `nv-lang/nova-compress`) уже опубликован и содержит рабочий код (checksum-дедуп волна и др.), НО: (1) `nova-http` (внешний пакет) всё ещё импортирует `std.encoding.compress` вместо `nova-compress` — `nova-http/src/client/client.nv:38` (`import std.encoding.compress.{gzip_decode, zlib_decode, inflate, brotli_decode, CompressError}`) и `nova-http/src/error.nv:32` (`import std.encoding.compress.{CompressError}`); (2) дубликаты кода живут в main-репо — `std/src/encoding/compress/` (deflate/gzip/zlib/brotli/checksum) и `compiler-codegen/nova_rt/brotli/` (vendored lib) оба всё ещё существуют; (3) `detect_brotli()` в `compiler-codegen/src/test_runner.rs` (~:1258) жив и используется. Ветка `plan205-compress` (коммит `c6ca26330`, «205 Ф.1-Ф.2 (путь 1): compress выселен в пакет nova-compress») + `p205-endgame` существуют ЛОКАЛЬНО, но НЕ влиты в main. Plan 205 Ф.2-эндгейм (переключить потребителей на nova-compress + удалить дубликаты из main) потерян/не завершён. | Plan 205 / 178 (http) | **P1/P2** |
| `[M-161-blanket-conflict-diagnostics-missing]` | **Найдено аудитом планов ≥150 (2026-07-16).** Спека объявляет `E_DUPLICATE_PROTOCOL_IMPL` (тип не может реализовывать `Next[T]` для двух разных `T` одновременно, `spec/decisions/02-types.md:13852` D355 §4) и `E_BLANKET_CONFLICT` (конфликт двух blanket-методов с одним именем на одном протоколе, там же §5 + `spec/decisions/10-overloading.md:715` D-крестссылка); Plan 161 помечен «✅ CLOSED Ф.0-Ф.4 2026-06-15». Однако `grep -rn "E_DUPLICATE_PROTOCOL_IMPL\|E_BLANKET_CONFLICT" compiler-codegen/src/` = **0 совпадений** — ни один из двух кодов диагностики не реализован в чекере. Хуже: негативная фикстура `spec_tests/conformance/blanket_dup_neg.nv` (имя подразумевает EXPECT_COMPILE_ERROR на дубликат) молча превращена в ПОЗИТИВНЫЙ тест — файл содержит явный комментарий «(positive test, not a negative test — duplicate detection is a separate concern)». Это нарушение принципа «тест авторитетен» (test-conventions/dev-workflow): негативный тест был ослаблен/переписан вместо того, чтобы остаться красным индикатором недостающей диагностики. Требуется решение владельца: реализовать обе диагностики (закрыть D355 §4/§5 по-настоящему) ЛИБО формально ретрактировать их из спеки, и в любом случае вернуть `blanket_dup_neg.nv` к негативной семантике (или переименовать, если решение — ретракт). | Plan 161 / checker | **P1** |
| `[M-198-f5-conformance-subdir-verdict]` | **Найдено аудитом планов ≥150 (2026-07-16).** `docs/plans/wip/198-redo-notes.md` §«Ф.5 — ревизия подпапок `spec_tests/conformance/`» (задание владельца 2026-07-14): инвентарь каждой подпапки `conformance/*/` → вердикт одной из трёх категорий (законный отдельный CU / вернуть плоскими пирами в merged-CU / карантин-бага) с таблицей-вердиктом «в этот файл». Задание НЕ выполнено — файл обрывается на списке «кандидатов под подозрением» (`any_is/`, `cm_box/`, `d372_canonical/`, `lint/`, `plan70_1/`, `plan84/`, `consume_fixtures/`), таблицы-вердикта нет. | Plan 198 Ф.5 | P3 |
| `[M-d412-blob-view-mut-write]` | Найдено ревью-2 Плана 210 (2026-07-16): blob-view над .rodata (одиночный `embed()` D412 и будущий `embed_dir`) при mut-биндинге ЗНАЧЕНИЯ (не литерала) не копируется — D412-копия (emit_c ~26599) ловит только биндинг блоб-ЛИТЕРАЛА. `mut d = f_returning_blob(); d[0]=5` → запись в read-only страницу = SEGV. Push безопасен (realloc уводит в кучу), опасна in-place запись. Фикс-кандидаты: чекер-запрет in-place записи в blob-view / рантайм-метка view+copy-on-write / документированный контракт. | D412 / Plan 210 ревью | **P2** |
| `[M-ro-launder-nova-http-migration]` | **Найдено 2026-07-23 при закрытии [M-ro-launder-via-mut-binding] (Plan 224).** `nova-http` (соседняя репа `d:/Sources/nv-lang/nova-http`) содержит ≥345 сайтов L1-ro→mut launder'а (замер под финальным чекером Ф.1, видно транзитивно через `examples/flagship/aggregator` — напр. `server/server_router.nv:308,321`, `servernet/policy.nv:97`). Вне мандата исполнителя Plan 224 (чужой репозиторий) — миграция (тем же паттерном: предпочитать `mut`-параметр, `.clone()` с обоснованием) должна проводиться мейнтейнером nova-http отдельной волной, после того как эта волна (Plan 224) сольётся в main и станет доступна как обновление зависимости компилятора. | nova-http (внешняя репа) | **P2** |
| `[M-ro-launder-bound-aware-scalar-analysis]` | **Найдено 2026-07-23 при доработке return-позиции Plan 224.** Чекер `[M-ro-launder-via-mut-binding]` не распознаёт generic bound-тип-сеты, ограничивающие `T` только скалярами (`fn[T SignedInts] f(x T) -> T => x`), как proof scalar-safety — `is_bare_scalar_primitive` смотрит только на КОНКРЕТНЫЙ резолвнутый тип, не на bound generic-параметра. Следствие: 12+ generic identity-подобных функций в `spec_tests/conformance` потребовали ручного `mut`-параметра (Plan 224 §4), хотя семантически их `T` ВСЕГДА скаляр. Опциональный follow-up: bound-aware анализ (проверить, что ВСЕ типы в bound'е type-set'а — скаляры) убрал бы эту ручную работу для будущего generic-кода. Не блокирует — текущий ручной `mut`-паттерн корректен и достаточен. | чекер (`types/mod.rs`, generic bound resolution) | **P3** |
| `[M-mut-params-registry-overload-conflation]` | **Найдено 2026-07-23 при реализации Plan 224 Ф.1 (call-argument позиция).** `ConsumeRegistry.fn_mut_params`/`method_mut_params` (`types/mod.rs`) ключуются по ИМЕНИ функции/метода, не по конкретному overload'у — для D326 mode-overload (`fn f(x T)`/`fn f(mut x T)`/`fn f(consume x T)`, один name) это конфлирует ВСЕ формы под одним ключом (last-decl-wins). Для `[M-ro-launder-via-mut-binding]`'s `check_readonly_coerce_args` это исправлено новыми `fn_overload_names`/`method_overload_names` guard'ами (пропуск проверки на overloaded именах). **Тот же корень НЕ проверен** для `check_unsafe_coerce_args` (аналогичная структура, `[M-118.5-arg-coerce-unsafe]`) — потенциальный симметричный false-positive/false-negative класс на overloaded именах с `unsafe T`-параметрами, не исследовано. | чекер (`types/mod.rs`, `check_unsafe_coerce_args`) | **P3** |

- **[M-mn-spawnctx-corruption-cancel-wake]** — **✅ РЕШЕНО 2026-07-19
  (opus-волна, worktree `nova-187w`, ветка `p-spawnctx-root`; полный разбор —
  `docs/plans/wip/211-spawnctx-notes.md` §«КОРЕНЬ НАЙДЕН И ЗАКРЫТ»).**
  «Порча SpawnCtx» оказалась СИМПТОМОМ. Корень: `GC_set_push_other_roots`
  (fiber_arena.c, введён ea85229e0) ЗАМЕЩАЛ дефолтный колбэк bdwgc, который
  на pthreads-сборке (`GC_default_push_other_roots` → `GC_push_all_stacks()`)
  — единственный канал сканирования СТЕКОВ И РЕГИСТРОВ всех потоков.
  Linux-порт Windows-модели перенёс лишь 1 из 3 слагаемых Windows-колбэка
  (занятые fiber-слоты), потеряв native-стеки потоков и стек main → всё
  рутованное только стеком (stack-локальный supervised `q`, его
  child_error[]/child_ctx[], локали шедулера, токен) собиралось GC ЖИВЫМ,
  страницы перекраивались — обе gdb-сигнатуры («32-битные усечения» =
  легитимные int32-записи рантайма в свой же отобранный массив; ASCII в
  SpawnCtx = страница ушла под строки) и рваный fail-top. Доказательства:
  PIN2-бисекция (дубль-достижимость live-массивов через uncollectable-цепь)
  10/10 PASS против 0/30 базлайна; mmap-вынос массивов 10/10; poison/карантин
  ручных free — эффекта нет; наивный чейнинг дефолта — SIGSEGV в GC-маркере
  (sp приостановленных воркеров внутри коро-стеков → диапазон через guard).
  Фикс: полная компенсация в `_nova_gc_push_other_roots` — (1) main-стек
  (probe + текущая VMA из /proc/self/maps на каждой сборке), (2) реестр
  native-стеков воркеров/драйвера (`nova_fiber_arena_register_native_stack`),
  (3) fiber-слоты как было; + bootstrap-страховка probe. Windows поведенчески
  не тронут (там компенсация была полной с Plan 151/Ф.2). Верификация:
  pos_max_fibers_concurrent 30/30 release + 30/30 dev (было 0/30);
  supervisor_stop_test 10/10; supervisor_parfor_test (known-red CI) 10/10.
  Оставлен opt-in диагностический инструментарий (ноль оверхеда без env):
  `NOVA_SPAWN_POOL_DIAG=1` (R1-трипваер пула: poison+канарейка+карантин+
  double-release-abort+live-проверки goready/resume/sweep/driver),
  `NOVA_UNCOLL_QUAR=1` (карантин-дискриминатор uncollectable-free). Прежние
  два сonnet-фикса (ACQUIRE-load count в driver.c; child_ctx[] collectable)
  остаются в силе как независимые улучшения.
- **[M-linux-mn-conformance-red]** (2026-07-16, P1, найден ПЕРВЫМ Linux-прогоном
  conformance — nova-gate CI, run 29513225018) — 2 фикстуры RUN-FAIL только на
  Linux (Windows зелёные): `app_effect_basic_t8_1` (63s, тесты внутри PASS,
  процесс падает на выходе) и `standalone/supervisor_parfor_test` (2-й тест
  supervisor+parallel-for). **2026-07-18: known-red-список расширен ещё двумя
  фикстурами того же семейства — см. [M-mn-spawnctx-corruption-cancel-wake]
  выше (точный gdb-корень).** Класс = подтверждённые TSan-гонки M:N baseline
  (runq init↔steal visibility gap; sysmon↔worker `runtime.c:615/1082`) — см.
  План 211 §5 (там же алгоритм). CI-гейт nova-gate.yml несёт known-red-список
  с этими 2 путями (НЕ ослабление тестов: любой другой FAIL = красный);
  снять список ВМЕСТЕ с фиксом гонок в 211. Конкретное CI-репро для 211 готово.
  **2026-07-17 (План 211 §7, sonnet):** из 3 TSan-подтверждённых гонок 2
  (sysmon↔worker preempt_flag, `alloc_boehm.c:110` counter) закрыты мелкими
  atomic-фиксами (TSan-верифицировано 0/6 прогонов после, worktree `nova-211r`).
  Третья (`runq init↔steal`) — архитектурная, дизайн готов (План 211 §7.3:
  расщепить `_materialize_pool` на init-фазу + spawn-фазу, Go/Tokio-прецедент),
  НЕ применена — решение владельца. Список known-red **остаётся без изменений**
  до применения+верификации §7.3 — связь «эти гонки → именно эти 2 фикстуры»
  пока классовая (не проверена прямым TSan-прогоном именно этих 2 фикстур,
  вне бюджета этого захода, см. План 211 §7.5).
  **2026-07-20 (диагностика WSL, sonnet, worktree `nova-appeffect`, ветка
  `p-fix-linux-appeffect`) — три находки, known-red СПИСОК НЕ ТРОНУТ (флака
  НЕ подтверждена мёртвой; `app_effect_basic_t8_1` остаётся единственной
  записью):**
  1. **Архитектурная находка (главная).** `app_effect_basic_t8_1.nv` —
     НЕ маленький изолированный тест. `spec_tests/conformance/` — плоский
     `module spec_tests.conformance` (993 co-equal файла); `test_runner.rs`
     (`walk_nv_selected`, доккомент у `TEST_RUN_INCLUDE_SLOW`) группирует
     co-equal файлы folder-модуля в ОДИН compile-unit, представленный
     АЛФАВИТНО-ПЕРВЫМ файлом группы. `app_effect_basic_t8_1` — этот
     представитель для ВСЕХ 993 файлов конформанса (тысячи test-блоков,
     включая concurrency-тяжёлые: "300 fibers spawn + sum",
     `NOVA_FIBER_STACK` многофайберные, и т.д.) — САМЫЙ БОЛЬШОЙ
     test-бинарь во всём наборе (~117s против ~10s у соседей на этой
     машине). Это объясняет, почему прошлые волны (211/spawnctx/187-wedge)
     не смогли изолировать его как `pos_max_fibers_concurrent`
     (тот живёт в МЕНЬШЕМ отдельном модуле `standalone/`) — и почему
     именно этот "тест" статистически чаще всего ловит редкие M:N-гонки
     (наибольшая экспозиция, не что-то специфичное в его тривиальном
     21-строчном содержимом).
  2. **Флака ЖИВА, но НЕ переизолирована однозначно.** Прямые прогоны
     `nova test spec_tests/conformance/app_effect_basic_t8_1.nv` на WSL2
     Ubuntu (собственный релиз-бинарь, native-fs checkout) — 5 прогонов и
     `--jobs 4`, и `--jobs 1` (полностью serial, чтобы исключить гонку
     УРОВНЯ test-harness) — RUN-FAIL в 4/5 (один PASS). Симптом:
     `RUN-FAIL ... app_effect_basic_t8_1.nv:22: assert failed:
     27.0.cbrt() == 3.0` — НО у файла всего 21 строка и НЕТ cbrt() —
     подставляется провал совершенно ДРУГОГО файла
     (`d109_primitive_methods_f64_f32_math.nv`, реально существующая
     строка 22) под чужой label. `--jobs 1` НЕ убирает нондетерминизм →
     это НЕ класс `[M-218-rt-archive-parallel-jobs-race]` (тот фикс уже
     смёржен в main, подтверждено `git merge-base --is-ancestor
     313ecc289 HEAD`) — похоже на гонку ВНУТРИ одного процесса
     (M:N-рантайм самого мега-бинаря, 16 воркеров на этой машине), не на
     уровне test-runner'а. Корень НЕ локализован (не хватило бюджета;
     `fiber_arena.c`/`alloc_boehm.c` — зона параллельной Boehm-волны,
     туда не заходил). Историческая сигнатура «тесты внутри PASS, падает
     на выходе» (crash) НЕ подтверждена и НЕ опровергнута напрямую — то,
     что видел я (assert-mismatch с чужим label), может быть ДРУГИМ
     проявлением той же гонки (shared/global «текущий тест» state для
     assert-репортинга) либо отдельным симптомом с тем же представителем.
  3. **Побочная находка — WSL-окружение обогнало toolchain, на который
     рассчитан nova_rt.** Эта WSL2 Ubuntu 26.04 сейчас несёт gcc 15.2.0 /
     clang 21.1.8 (новее, чем на момент верификации `docs/linux-build.md`
     2026-07-16 и волны 187-wedge/211 два дня назад — видимо, rolling-release
     обновился). `libnova_rt`-архив (Plan 218, кэшируемый) НЕ собирается
     ни gcc, ни clang: `sync_primitives.h`/`bench.h` (gcc: `__atomic_fetch_and`
     на `_Bool*`, pointer-type ternary) + `deque.h`/`typeid.h` (clang:
     implicit-declaration errors, C23-режим строже) — заставляет ВСЕГДА
     падать в "per-build inline compile" fallback (путь, который CI,
     видимо, никогда не проходит — там toolchain совместимее). Это
     означает: мои прогоны идут по РЕДКО ИСПОЛЬЗУЕМОМУ коду, ЧТО САМО ПО
     СЕБЕ confound для сопоставления с CI-сигнатурой. Почин исправлен
     (`deque.h` — недостающий `#include <stdlib.h>`, тривиально/безопасно,
     Windows-гейт `standalone` PASS 69/FAIL 0 подтверждает отсутствие
     регресса); `typeid.h` и, вероятно, другие заголовки — НЕ исправлены
     (глубже, вне бюджета этой волны, whack-a-mole риск). Для чистого
     повтора CI-условий на WSL нужна отдельная волна: либо докрутить
     недостающие `#include` по всем `nova_rt/*.h` (безопасно, но
     трудоёмко-методично), либо запиновать toolchain постарше.
  Вывод: known-red список НЕ меняю (флака не мертва); фикс M:N-гонки НЕ
  предпринят (root cause не локализован в рамках зоны/бюджета — предпринимать
  спекулятивный фикс без верифицированного корня запрещено §4а). Единственная
  правка — `deque.h` include-гигиена (нулевой риск, не concurrency-логика).
  **2026-07-20 (продолжение, sonnet, worktree `nova-rtheaders`, ветка
  `p-fix-rt-headers`) — include-гигиена по ВСЕМ `nova_rt/*.h` завершена,
  вскрыт отдельный НЕ-include класс gcc-ошибок:**
  Аудит всех 31 заголовка `compiler-codegen/nova_rt/*.h` (кроме vendored
  `libuv/`) на self-containedness (grep реального использования libc-символов
  vs собственных `#include`, каждое совпадение верифицировано вручную против
  ложных срабатываний в комментариях). Исправлено 9 файлов (только
  `#include`-строки, нулевой риск): `channels.h`/`nova_sched.h` (+`stdio.h`
  +`stdlib.h`: `fprintf`/`abort`/`malloc`/`free`/`getenv`/`atexit`),
  `sync_barrier.h`/`sync_condvar.h`/`sync_countdown_latch.h`/`sync_semaphore.h`
  (те же 2 инклуда — раньше ВООБЩЕ без своих `#include`, полагались
  транзитивно на `sync_primitives.h`, который их `#include`-ит по факту
  реальными директивами, не text-splice), `typeid.h` (+`string.h` для
  `memcpy`, +`"alloc.h"` для `nova_alloc` — последнее найдено ПРЯМОЙ
  компиляцией `typeid.c`, единственного `.c` из `rt_archive_sources`, кто
  инклудит `typeid.h` без `nova_rt.h`-bootstrap: gcc 15 давал `implicit
  declaration of function 'nova_alloc'` → `int→void*` `-Wint-conversion`),
  `vtables.h` (+`string.h` для `memcpy`, +`stdbool.h` для `bool` в его
  standalone-fallback `#ifndef NOVA_RT_H` typedef-блоке),
  `plan115_ffi_test.h` (+`string.h` для `strlen`). Остальные 22 заголовка
  (включая `sync_primitives.h`, `bench.h` — оба ранее подозревались) уже
  self-contained: либо через собственный `#include "nova_rt.h"`-bootstrap
  (`fibers.h`/`effects.h`/`bench.h`/`net.h`/`fs.h`), либо полный свой список.
  **Гейты:** Windows (`cl.exe`, реальный prod-toolchain) — `cargo build
  --release` чисто; `NOVA_RT_ARCHIVE=1` архив с нуля собрался
  (`libnova_rt.lib built (13 files)`); `spec_tests/conformance/standalone`
  **PASS 68/FAIL 0** (вкл. `pos_max_fibers_concurrent`,
  `supervisor_parfor_test`). WSL2 та же машина (gcc 15.2.0/clang 21.1.8),
  ручная репликация `build_rt_archive_lib`'s Unix-ветки (идентичные флаги):
  **clang — ARCHIVE_OK** (все 13 `.c` чисто, `ar rcs` собрал `libnova_rt.a`;
  include-фиксы полностью закрывают clang-класс из этой заметки). **gcc —
  ВСЁ ЕЩЁ FAILED**, но `typeid.c` теперь чист; остаются 3 категории ошибок,
  ни одна НЕ include-related (полная категоризация в
  `docs/plans/wip/rt-headers-notes.md`): (1) `struct NovaFiberQueue*` vs
  `NovaFiberQueue*` type mismatch (44 error) — `driver.h` форвард-декларирует
  ТЕГИРОВАННЫЙ `struct NovaFiberQueue;`, `fibers.h` определяет АНОНИМНЫЙ
  `typedef struct {...} NovaFiberQueue;` — два разных C-типа, пронизывает
  `driver.h`/`driver.c`/`fibers.h`/`runtime.h`; аналогично
  `struct NovaBlockingState*`; (2) `__atomic_fetch_and/or/xor` на
  `nova_atomic_bool*`(`_Bool*`) в `sync_primitives.h` (36 error) — ЭТО И ЕСТЬ
  ранее задокументированный gcc-флаг выше, НЕ include (файл уже полный);
  (3) pointer-type mismatch в тернарнике (`const uint8_t*` vs `char*`
  string-literal) в `effects.h::nv_exit` + `bench.h::nova_bench_emit_metric` —
  ЭТО И ЕСТЬ «pointer-type ternary» флаг выше. Root cause расхождения:
  GCC 14+ (в т.ч. 15.2 здесь) продвинул `-Wincompatible-pointer-types` из
  warning в error-by-default для C; prod-флаг `-w` это НЕ подавляет
  (verified). Clang 21 на идентичном источнике+флагах эти диагностики не
  эскалирует. **Ни одна из 3 категорий НЕ тронута** — правка сверх
  `#include`-строк (struct-tag unification / `_Bool`-atomic redesign /
  ternary cast-fix) запрещена мандатом этой волны (§4а, «СТОП+доклад» вместо
  спекулятивного фикса вне зоны). Требует отдельной волны с явным решением
  владельца (или пиновка toolchain постарше для CI-паритета, см. п.3 выше).
  **2026-07-20 (закрытие, sonnet, worktree `nova-gcc15`, ветка
  `p-fix-gcc15-rt`) — все 3 категории gcc15-ошибок исправлены, gcc15-подпункт
  ЗАКРЫТ (полная деталь в `docs/plans/wip/gcc15-rt-notes.md`):**
  (1) struct-tag unification — `typedef struct {...} NovaFiberQueue;` /
  `NovaBlockingState` в `fibers.h` получили тег (`typedef struct
  NovaFiberQueue {...} NovaFiberQueue;` и аналогично для `NovaBlockingState`),
  унифицирован с тегированной форвард-декларацией в `driver.h` — ABI/layout
  не меняется, чисто source-level. (2) `_Bool`-atomic RMW — `nova_atomic_bool`
  underlying-тип НЕ менялся (остался `bool`; 15+ scheduler-сайтов —
  cancel_requested/stop/started/published/done/cancelled/closed — не
  тронуты); переписаны ТОЛЬКО 6 stdlib-методов
  `Nova_AtomicBool_method_fetch_{or,and,xor}_{bool,MemOrdering}`
  (`sync_primitives.h`, единственное место в nova_rt с битовым RMW на
  `nova_atomic_bool`, никогда не вызывается из scheduler-кода) на
  load+CAS-retry-loop — тот же идиом, что уже используется в этом файле для
  `fetch_max`/`fetch_min`. (3) pointer-mismatch ternary — точечные касты
  `(const uint8_t*)""`/`(const uint8_t*)"?"` в `effects.h::nv_exit` и
  `bench.h::nova_bench_emit_metric` (голый `""`-литерал — `char*` в C, а
  `nova_str.ptr` — `const uint8_t*`). **Гейты:** WSL2 gcc 15.2.0 — ARCHIVE_OK
  (все 13 `.c` чисто); WSL2 clang 21.1.8 — ARCHIVE_OK (не сломан). Windows
  (`nova-cli`) — `cargo build --release` чисто; `NOVA_RT_ARCHIVE=1` архив с
  нуля собрался (13 files); `spec_tests/conformance/standalone` **PASS
  70/FAIL 0** (вкл. `pos_max_fibers_concurrent`/`supervisor_parfor_test`/
  `supervisor_stop_test`); `pos_max_fibers_concurrent`+`supervisor_stop_test`
  — **5× подряд PASS** (планировщик/атомики не сломаны); флагман-агрегатор
  под `--strict-effects` собрался и ответил `HTTP 200` на `curl` живого
  сервера. Мега-CU не гонял. **Итог: rt-архив (Plan 218) теперь собирается
  чисто на обоих WSL2-toolchain'ах** — fallback на per-build inline compile
  для этой машины больше не нужен. В main не смёржено, не запушено —
  решение владельца.
  **2026-07-20 (ЗАКРЫТИЕ, sonnet, worktree `nova-linuxrace`, ветка
  `p-fix-linux-mn-red`) — маркер ЗАКРЫТ, known_red снят из
  `nova-gate.yml`.** Вердикт: `app_effect_basic_t8_1` НЕ был жертвой
  M:N-гонки — ТРИ независимых, полностью ДЕТЕРМИНИРОВАННЫХ дефекта,
  впервые вскрытых end-to-end прогоном на WSL2 (свежий main, rt_archive
  default ON — ПЕРВЫЙ реальный CI-эквивалентный прогон этой комбинации на
  Linux; предыдущие волны либо гоняли per-build inline compile fallback,
  либо только компилировали архив вручную без реального `nova test`):
  (1) **link-order баг** (`test_runner.rs::build_command`, Clang+Gcc Unix
  branches) — `libuv.a` добавлялась в командную строку линковки ДО
  `opts.c_file`/`libnova_rt.a`, которым нужны её символы (`uv_strerror` и
  пр., из `fibers.h`) — GNU `ld` резолвит архив только против СИМВОЛОВ,
  undefined НА МОМЕНТ его появления в команде; ссылки, возникшие ПОЗЖЕ, не
  ищутся повторно. CC-FAIL `undefined reference to uv_strerror`. Фикс:
  переставлены object/library-аргументы (libuv теперь строго ПОСЛЕ
  ссылающихся объектов), Windows-ветка не тронута (MSVC linker не
  order-зависим). (2) **`build_rt_archive_lib` (Plan 218) Unix-ветка не
  передавала `-ffunction-sections`/`-fdata-sections`** — без per-function
  секций финальный `--gc-sections` не мог вычистить МЁРТВУЮ
  `nova_bench_heap_sampler_thread` (её globals определяются ТОЛЬКО в
  bench_mode, emit_c.rs:7174) из архивных `.o`, тянула неразрешённые ссылки
  в обычную (non-bench) сборку — CC-FAIL
  `_nova_bench_heap_sample_interval_ns`/`_nova_bench_heap_sampler_stop`.
  Windows-половина той же функции уже имела эквивалент (`/Gy`) — асимметрия
  Unix/Windows. Фикс: добавлены оба флага. (3) **cbrt non-portability**
  (`spec_tests/conformance/d109_primitive_methods_f64_f32_math.nv:24,56`) —
  `assert((27.0).cbrt() == 3.0)` полагался на exact equality; IEEE-754 НЕ
  гарантирует correctly-rounded `cbrt` (в отличие от `sqrt`) — glibc's
  runtime `cbrt(27.0)` на этой машине даёт `3.0000000000000004441` (1 ULP);
  на Windows/MSVC и через GCC's compile-time constant-folder (ТОЛЬКО для
  литералов, `pow(x,1/3)`-путь variable-формы всё равно бы упал) исторически
  давало ровно `3.0` — платформенно-хрупкий assert, деterministически падал
  на Linux+clang (Auto-toolchain-preference = Clang>Gcc на Linux). Фикс:
  оба assert'а (f64-литерал + f32-переменная) переведены на epsilon-сравнение
  (`1e-9`/`1e-5`). Побочная находка (НЕ починена, вне зоны — заведён
  отдельный маркер `[M-emit-c-loc-for-span-wrong-file-merged-cu]` ниже):
  компиляторный баг мисатрибуции file:line для folder-module merged CU
  (`emit_c.rs::loc_for_span` — `self.source_file_name`/`annotation_source`
  process-global вместо per-span originating file; `byte_to_line_col` без
  bounds-check даёт детерминированный garbage-line при overshoot) — это и
  объясняло исторический сбивающий с толку label «app_effect_basic_t8_1.nv:22»
  для assert'ов, реально находящихся в d109 на строках 24/56. Побочно
  улучшена диагностика `test_runner.rs`: `NOVA_DEBUG_CC_DUMP=1` (полный
  stdout/stderr дамп при CC-FAIL, ноль оверхеда без env, мирроринг
  `NOVA_DEBUG_TIMEOUT_DUMP`) + расширен `errs`-фильтр (раньше матчил только
  substring "error" — GNU ld'шные `undefined reference to`/`cannot find -l`
  строки не содержат слово "error", терялись за уводящей в сторону
  clang-обёрткой "linker command failed"). **Гейты:** WSL2 представитель
  (rt_archive default ON, реальный CI-путь) — **20/20 PASS подряд**, ноль
  крэшей/SIGSEGV/зависаний. Windows: `cargo build --release` чисто;
  `spec_tests/conformance/standalone` **PASS 70/FAIL 0**;
  `pos_max_fibers_concurrent`+`supervisor_stop_test`+`supervisor_parfor_test`
  **×5 подряд PASS**; флагман-агрегатор собрался под `--strict-effects` и
  ответил `HTTP 200` живому `curl`. **known_red-строка снята из
  `.github/workflows/nova-gate.yml`** (заменена closure-комментарием с
  диагнозом). Полная деталь — `docs/plans/wip/linux-mn-red-notes.md`.
  Файлы: `compiler-codegen/src/test_runner.rs`,
  `spec_tests/conformance/d109_primitive_methods_f64_f32_math.nv`,
  `.github/workflows/nova-gate.yml`. Worktree `nova-linuxrace`, ветка
  `p-fix-linux-mn-red` — в main НЕ смёржено, не запушено, решение
  интегратора.

- **[M-replace-transitive-deps]** (2026-07-16, P3, найден compress-lock волной 205 Ф.2) —
  `[replace]` в nova.local.toml действует ТОЛЬКО на `[dependencies]` корневого пакета
  сборки (go-семантика), транзитивные зависимости (`http → compress`) НЕ перекрывает
  (`W_REPLACE_UNKNOWN_DEP`). Для локальной разработки транзитивного пакета нужен
  либо прямой dep в корне, либо расширение replace-семантики (дизайн-вопрос D420-семье).

- **[M-embed-dir-self-embed-reject]** (2026-07-16, P3, ревью-3 Плана 210) —
  `embed_dir(".")`/`""` (self-embed корня пакета, включая исходники) не отвергается
  явно; решить: E_ или осознанное разрешение. Открытая развилка владельца вне
  таблицы кодов §4.3 плана 210.

- **[M-z3-contracts-filter-misses-soundness]** (2026-07-16, P3, побочная находка
  fix-z3-soundness-guard) — jobs contracts-trivial/contracts-z3 фильтруют
  `nova test . --filter contracts` подстрокой пути; `spec_tests/soundness/` её не
  содержит → soundness-фикстуры больше не гоняются через реальные backend'ы
  (jobs non-blocking). Anti-delete ratchet работает; content-верификация — нет.

- **[M-time-folder-coequal-mismatch]** — **✅ РЕШЕНО 2026-07-17 (Plan 211, ветка
  `fix-module-layout-orphan`, worktree `nova-modlayout`).** Layout-фикс: `git mv
  std/src/time/duration.nv → std/src/time/duration/core.nv` +
  `timestamp.nv`/`monotonic.nv` → `std/src/time/duration/{timestamp,monotonic}.nv`
  (чистые renames, `module time.duration` не менялся, import-путь
  `std.time.duration` не менялся). Теперь `duration/` — настоящая папка-модуль
  (D78 «файл ИЛИ папка»): `is_folder_module_peer` (imports.rs:1999-2053) видит
  ЕДИНУЮ декларацию на папку с последним сегментом == имя папки (`duration` ==
  `duration`) → условие выполняется. Подтверждено: `nova check` на
  `std/src/time/duration/timestamp.nv` и `.../monotonic.nv` как ПРЯМЫХ entry —
  чисто (было `E_D78_MODULE_PATH_MISMATCH`). Попутно откачен маскирующий фикс
  `[M-blanket-crossmodule-scattered-peer-drop]` (59f22a85b, противоречил букве
  D78 — «сколько угодно co-equal файлов в общей папке» вместо «файл ИЛИ папка»)
  и заведена громкая диагностика `[M-module-file-submodule-split-silent-orphan]`
  (см. ниже) на месте того класса тихого сиротения, который маскирующий фикс
  пытался обойти silent-мёржем. `std/src/time/civil/parse_test.nv` (исходный
  cross-module репро) — `to_unix_seconds`/`to_unix_nanos` E_UNKNOWN_METHOD
  исчезли, остаётся только НЕСВЯЗАННЫЙ pre-existing `[E7301]` FmtKind/Month в
  `tz.nv` (чинится параллельно, не тронут). Попутная улика (ICE emit_c.rs:52222
  `[P67-LEGACY]` method=now/to_nanos) РАЗОБРАНА и заведена ОТДЕЛЬНО —
  `[M-checker-path-call-chain-unknown-ret-type]` (см. ниже) — НЕ то же самое,
  что layout-баг, была лишь попутно замечена в той же волне-208. Было
  (исходный OPEN-контекст, 2026-07-16, P2, найден 208-фикс-волной после П13
  file-split): `std/src/time/` содержал module `time.duration` в 3 co-equal
  файлах (duration/timestamp/monotonic.nv), разбросанных РЯДОМ с `time.cron`
  (cron.nv) прямо в `std/src/time/` — `is_folder_module_peer` требовал ЕДИНУЮ
  декларацию на папку с последним сегментом == имя папки, что не выполнялось
  ни для одного файла → `E_D78_MODULE_PATH_MISMATCH` при компиляции
  monotonic/timestamp как прямых test-энтри.

- **[M-module-file-submodule-split-silent-orphan]** — **✅ РЕШЕНО 2026-07-17
  (Plan 211, ветка `fix-module-layout-orphan`).** Резолвер модулей
  (`compiler-codegen/src/imports.rs::resolve_module_paths`, зона резолвера, НЕ
  frozen emit_c) теперь громко диагностирует тихое сиротение file-submodule:
  если plain-импорт резолвится в единственный head-файл `<Y>.nv` (файловый
  модуль, D78), но в ТОЙ ЖЕ директории лежат ещё `.nv`-файлы, объявляющие ТОТ
  ЖЕ `module X.Y` (co-equal peers, разбросанные напрямую в общем родителе
  вместо выделенной папки-модуля `Y/`) — раньше это было тихим сиротением (до
  Plan 202 peer-декларации молча выпадали из любого внешнего резолва; после
  временного маскирующего фикса `[M-blanket-crossmodule-scattered-peer-drop]`
  — молча подмешивались без диагностики). Теперь `ResolveErr::FileOrphan` →
  `[E_MODULE_FILE_ORPHAN]` compile error, 4 части (какой файл+что объявил /
  почему не входит / следствие-невидимость / fix-подсказки папка-модуль).
  Neg-фикстура `spec_tests/conformance/neg/module_file_orphan.nv` (+
  `module_file_orphan_repro/{core,scattered}.nv`) — package-scale репро
  реального std/time-бага, подтверждено `nova test ... --full` → PASS
  (negative). 4 существующих юнит-теста imports.rs (`entry_folder_module_tests`)
  — зелены, не задеты (диагностика — отдельная ветка кода, unrelated к
  entry-sibling-scan).

- **[M-checker-path-call-chain-unknown-ret-type]** (2026-07-17, P2, найден Plan
  211 при верификации `nova test std/src/time` после layout-фикса, разблокировавшего
  `std/src/time/duration/monotonic.nv` для реальной компиляции извне) —
  `nova: internal error at compiler-codegen/src/codegen/emit_c.rs:52222:
  [P67-LEGACY] Path call return type unknown for method=<name> — checker must
  annotate`. Триггер — метод, вызванный ЦЕПОЧКОЙ сразу на РЕЗУЛЬТАТЕ вызова
  функции/статик-пути (`<free_fn_or_static_call>(...).method()`), а НЕ на
  переменной/литерале: конкретно `monotonic.nv:105`
  `sat_sub_i64(@nanos, other.nanos, i64.MIN, i64.MAX).to_nanos()` внутри
  `Monotonic @elapsed_since` (и транзитивно `@minus`/`@checked_duration_since`)
  — чекер не аннотирует тип возврата promежуточного «Path call» до чейн-метода.
  Репро: `nova test std/src/time/overflow_safe_test.nv --full` (тест
  «Ф.1c/D318: Monotonic non-regression», вызывающий `elapsed_since`) — ICE.
  Контраст: изолированная копия `d317_duration_overflow_policy.nv` (не
  трогает Monotonic, только Duration/Timestamp через литералы/переменные,
  НЕ через промежуточный free-fn-call-чейн) — PASS чисто. Значит баг НЕ в
  layout-фиксе и НЕ в свежесвязанной cross-module видимости per se — это
  ортогональный, ранее НИКОГДА не упражнявшийся codegen/checker-гэп в самой
  реализации `Monotonic.elapsed_since`, вскрытый ТЕМ, что модуль наконец
  компилируется извне целиком (был анонсирован тем же 208-фикс-волна попутно
  найденным «ICE emit_c.rs:52222 method=now» в предшественнике этой строки,
  `[M-time-folder-coequal-mismatch]` — та же общая природа: return-type
  inference для «Path call» перед чейн-методом; возможно другой конкретный
  call-сайт с `.now()`, не найден в текущей волне, не искали specifically).
  Не чинить в рамках layout-волны (out of scope, отдельный checker/codegen
  заход) — зона: `types/mod.rs` return-type inference для call-expression
  ИЛИ `emit_c.rs` P67-LEGACY fallback-путь (~52222).

- **[M-static-conv-array-record-mono-cc-fail]** (2026-07-17, P2, найден rtlint-волной
  при W_STATIC_CONVERSION-ретракции; РАЗВЕДКА-волна 2026-07-17 НЕ закрыла) —
  extension-метод с ресивером `[]u8` и user-record телом: `fn []u8
  @to_readbuffer() -> ReadBuffer { ReadBuffer { ... } }` (симметрично
  to_writebuffer). Из-за этого канон-переименования `ReadBuffer.from`/
  `WriteBuffer.from` → `x.to_*()` ОТКАЧЕНЫ и стоят под `nova:allow
  W_STATIC_CONVERSION` (read_buffer.nv:54, write_buffer.nv:60). После фикса —
  вернуть переименования и снять оба подавления. Детали: wip/lint-zero-notes.md.

- **[M-str-from-migration-byte-str-confusion]** (2026-07-17, P2, вскрыт untyped-let-волной)
  — при миграции `str.from(x)` → `x.to_str()` в nova_tests/generics/plan101_1_vec_map_int_str.nv
  вскрывается codegen-путаница byte/str (глубже, чем сам retract); правка откачена,
  файл не тронут. Разобрать отдельным заходом; блокирует полную миграцию
  nova_tests/generics на пост-174.2 API.
  to_writebuffer) ломает НЕСВЯЗАННУЮ типизацию в том же compile unit
  (воспроизведено дословно: `[E7320] no field or method ptr/len on []u8` в
  СОВЕРШЕННО другом файле `string/core.nv`, метод `to_str_unchecked`; в
  других запусках — `[E_RECV_METHOD_MISMATCH]` на ресивере `HashMap`/
  `WriteBuffer` — см. ниже про нестабильность). Разведка-волна (worktree
  `nova-slicemono`) прочесала `emit_c.rs` is_array_ext-регистрацию,
  `check_instance_overload`'s `array_elem_key` (196.7-класс), Channel 1/1b в
  `infer_expr_c_type`, `sig_registry.rs::merge_module_fns`'s per-type
  «already known» гейт (точечный фикс на per-(type,method) ПРИМЕНЁН И
  ПЕРЕПРОВЕРЕН — не помог, откачен) — корень НЕ локализован до одной строки.
  КРИТИЧЕСКАЯ методологическая находка: `external_registry.rs`'s
  `include_str!("../../../std/src/runtime/read_buffer.nv")` не всегда
  корректно инвалидируется Cargo incremental build при правке ТОЛЬКО .nv-файла
  — обязателен `touch compiler-codegen/src/codegen/external_registry.rs` перед
  каждым rebuild после правки read_buffer.nv/write_buffer.nv/etc., иначе
  результаты недостоверны (без этого 3 разных rebuild дали 3 разных ложных
  симптома). Полный протокол расследования, отвергнутые/неподтверждённые
  кандидаты, карта для следующей волны, владелец-директива по финальной форме
  API (to_*/into_* dual-form) — **docs/plans/wip/slice-ext-record-notes.md**.
  Канон-переименования `ReadBuffer.from`/`WriteBuffer.from` → `x.to_*()`
  ОСТАЮТСЯ откачены, стоят под `nova:allow W_STATIC_CONVERSION`
  (read_buffer.nv:54, write_buffer.nv:60). После фикса — вернуть
  переименования (dual-form to_*/into_*, см. notes) и снять оба подавления.

## [M-217-spawn-closure-consume-cleanup-undefined] — CI-гейт КРАСНЫЙ: `Nova_TcpStream_consume_cleanup` undefined внутри `_nova_spawn_0` (P0, 2026-07-21) — ✅ РЕШЕНО

**РЕШЕНО 2026-07-21 (sonnet, worktree `nova-spawncl`, ветка `p-fix-spawn-cleanup`):**
регрессия Plan 217 (авто-`@cleanup`, влит 22f3a519f + 45f047098), ловившая CI
на `echo_server_net`/`echo_client_net` (+ tls-пара транзитивно). Симптом:
`nova build examples/net/echo_server.nv --strict-effects` → `lld-link:
undefined symbol Nova_TcpStream_consume_cleanup`, 3 ссылки все внутри
`_nova_spawn_0` (паттерн `consume stream = conn` ВНУТРИ `spawn { }`).

Разведка вскрыла ДВА независимых дефекта в одной области:

**(a) DCE-семя пропущено для bare-consume-let.** `compiler-codegen/src/
lints.rs::collect_stmt`'s `Stmt::Let`-ветка (~975) не сеяла имя `"cleanup"`
для method-DCE reachability-замыкания — в отличие от соседней
`Stmt::ConsumeScope`-ветки (~1002, `out.insert("cleanup")`, добавлено ещё в
Plan 159.1/187 под ТОЧНО такой же паттерн для block-формы `consume x = e {
… }`). Bare-форма `consume x = e` (без `{ … }`, Plan 217 «гибрид C») тоже
диспетчит `@cleanup` через синтетический `Nova_<T>_consume_cleanup`-символ
(`enter_defer_scope`'s auto-cleanup prologue), никогда не пишется как AST
`Member`/`Call`-нода — но семени для неё не было. `(TcpStream, cleanup)`
пары никогда не «зажигались» → метод-DCE выпиливал ОПРЕДЕЛЕНИЕ (`nova
build`-исполняемые имеют `fn main` → DCE активен), а call-сайт (эмитится
AST-путём, независимо от DCE) продолжал на него ссылаться → undefined
symbol при линковке. **Фикс:** `if d.consume { out.insert("cleanup"...) }`
в `Stmt::Let`-ветке (lints.rs ~980-998). Regression-guard: Rust unit-тест
`bare_consume_let_seeds_cleanup_method` (emit_c.rs, `dce_tests`-модуль,
рядом с существующим `consume_scope_exit_seeds_cleanup_method`).

**(b) `emit_spawn` игнорировал auto-cleanup ПОЛНОСТЬЮ для bare consume-let
ПРЯМО в теле spawn (не вложенных в match/if).** Найдено ПРИ верификации
фикса (a) новой conformance-фикстурой — assert упал в рантайме
(`exit_calls == 0`, не 1) даже после фикса (a). Корень: `emit_spawn`'s
собственный statement-loop для тела (`emit_c.rs` ~12320,
`ExprKind::Block(b) => { for stmt in &b.stmts { self.emit_stmt(stmt)?; } }`)
НИКОГДА не вызывал `enter_defer_scope`/`leave_defer_scope` — единственное
место block-эмиссии во всём `emit_c.rs`, которое их пропускало (у
`emit_supervised`'s собственного тела, у match-арм, у `if`/`while`-тел —
везде есть). Без `enter_defer_scope`'s prologue-скана auto-cleanup-let
никогда не «взводился» (`active_var`/`consume_policy` не регистрировались)
— `@cleanup` молча НИКОГДА не запускался для этого конкретного размещения
(тихий resource leak, не просто отсутствующий символ). Echo-примеры не
задеты этим вторым дефектом — их `consume s = stream` вложен в `match … {
Ok(consume stream) => { … } }`-арм, а арм-тело эмитится ОТДЕЛЬНОЙ функцией,
которая enter_defer_scope зовёт как обычно. **Фикс:** обернуть
`ExprKind::Block(b)`-ветку `emit_spawn`'s body-эмиссии в `enter_defer_scope
(b, false)` / `leave_defer_scope(block_id)` — зеркало `emit_supervised`'s
уже существующего паттерна (~12630). Безопасно byte-identical для КАЖДОГО
существующего spawn-тела без defer/bare-consume-let (`enter_defer_scope`
рано возвращает `block_id=0` когда `!block_has_defers &&
!block_has_auto_cleanup_lets`, `leave_defer_scope(0)` — no-op); грепом по
всему `spec_tests/conformance` подтверждено — ни одна существующая
фикстура не держит top-level `defer` прямо в `spawn { }` (значит ни одна не
полагалась на старое молчаливое no-op поведение).

**Гейты (все 5 CI-целей, `--strict-effects`, worktree `nova-spawncl`):**
`echo_server_net`/`echo_client_net`/`echo_server_tls`/`echo_client_tls`/
`aggregator` — ВСЕ **built** (до фикса (a) — undefined symbol на net+tls
парах; после — зелёные). Точечный consume-regress (d432 + 3× d157/d180-ok +
7× d157/d180-neg, изолированный subset вне мега-CU) — **8/8 PASS**.
Rust `dce_tests` (24, включая новый) + `lints::tests` (86) — все ok.

Regression-guard: 8-й `test` в `spec_tests/conformance/
d432_auto_cleanup_hybrid_c.nv` (`"D432: bare consume-let inside spawn
closure auto-cleans exactly once"`) — гоняет ОБА дефекта функционально
(DCE не активен для `nova test`, у фикстуры нет `fn main` —
`compute_dead_decls_with`'s `has_main`-гейт; дефект (a) отдельно накрыт
Rust unit-тестом выше + флагман-гейтами).

## [M-217-break-continue-loop-boundary-bleed] — `break` внутри тривиального вложенного `loop{}` протекает cleanup во внешний scope (P0, 2026-07-21, follow-up к M-217-spawn-closure-consume-cleanup-undefined) — ✅ РЕШЕНО

**РЕШЕНО 2026-07-21 (sonnet, тот же worktree `nova-spawncl`, ветка
`p-fix-spawn-cleanup`, коммит поверх b017de1bb):** большой гейт на
объединённом main поймал регресс ПОСЛЕ фикса (b) выше:
`spec_tests/conformance/readguard_writeguard_separated.nv` тест «multiple
ReadGuards can coexist» — детерминированный RUN-FAIL «RwLock.read_unlock()
called without a matching read()» на всех 4 итерациях `parallel for`.

**Корень** (найден дампом реального `.c` из спойлер-верифицированного
прогона фикстуры, НЕ из синтетики): паттерн `consume rg = rw.read(); loop {
… if … { break } … }; rg.unlock()` (CAS-retry цикл МЕЖДУ consume-let'ом и
ручным disarm-вызовом). `enter_defer_scope` (emit_c.rs ~26040) ранним
`return 0` НЕ регистрирует scope для тела цикла, если у ЭТОГО тела нет
СВОИХ `defer`/auto-cleanup-let (чистая perf-оптимизация — CAS-retry `loop`
не имеет своих). Но `Stmt::Break`'s `emit_early_exit_cleanup(stop_at_loop=
true)` (~26652) идёт по `self.defer_scopes` СВЕРХУ в поисках БЛИЖАЙШЕГО
`is_loop_body`-маркера, чтобы там остановиться — раз у ломаемого цикла
маркера нет вовсе, проход «проскакивает» мимо (несуществующей) границы
цикла прямо во ВНЕШНИЙ, ещё открытый scope (`rg`'s auto-cleanup scope,
зарегистрированный фиксом (b) выше!) и ПРЕЖДЕВРЕМЕННО стреляет его
`@cleanup` — хотя внешний scope НЕ завершается этим `break` (он выходит
только из ВНУТРЕННЕГО `loop{}`). После break'а выполнение продолжается,
доходит до ручного `rg.unlock()` — ВТОРОЙ релиз того же guard'а →
double-release. Voспроизведено БЕЗ единого spawn'а (голый `fn main`,
`consume r = mk(); loop { … break … }; r.close()` → `CLEANUP CALLED` ДО
`CLOSE CALLED`) — доказывает: баг ОБЩИЙ, pre-existing в разделяемой
break/continue-машинерии, просто НИКОГДА раньше не упражнялся для
spawn/parallel-for тела (до фикса (b) spawn-тела вообще не регистрировали
реальный auto-cleanup scope — веткам просто нечего было «протекать»).

**Фикс:** новое поле `loop_body_has_scope: Vec<bool>` (emit_c.rs, рядом с
`auto_cleanup_active`) — `emit_loop_body_inline_ex` (ЕДИНСТВЕННая точка
входа для ВСЕХ форм цикла: for/while/loop — проверено грепом, единственный
вызов `enter_defer_scope(_, true)` во всём файле) пушит
`block_id != 0` ПЕРЕД телом, попает ПОСЛЕ. `Stmt::Break`/`Stmt::Continue`
теперь проверяют `loop_body_has_scope.last()`: `Some(false)` (ближайший
цикл НЕ зарегистрировал scope) → `emit_early_exit_cleanup` вообще НЕ
вызывается (безопасно: раз у цикла нет своего scope, внутри него по
построению не может остаться ничего открытого на `self.defer_scopes` —
всё, что было открыто ВНУТРИ, уже закрылось своим `leave_defer_scope` до
этой точки); `Some(true)`/пусто (defensive fallback) → старое поведение
(уже корректно останавливается на РЕАЛЬНО зарегистрированном scope цикла).

**Гейты (СИНХРОННО, worktree `nova-spawncl`):**
`readguard_writeguard_separated.nv` (запущен в реальной folder-CU локации,
`d256_contract_self_field.nv` временно вынесен из директории на время
верификации — у него ложно-срабатывающий `// REQUIRES_SMT_BACKEND` внутри
ПРОЗЫ комментария, наивный парсер маркера матчит его как директиву и молча
скипает ВЕСЬ top-level combined-module — pre-existing, НЕ этой волны
дефект, за periметром; d256 возвращён на место после верификации,
НЕ трогался) — **3/3 PASS** (детерминированно, до фикса — 4/4 RUN-FAIL,
воспроизведено дословно с тем же сообщением). d432-фикстура (9 тестов,
добавлен новый regression-test) + 3× d157/d180-ok + 7× d157/d180-neg —
**8/8 PASS** (repored-entries). Все 5 CI-целей (echo_server/echo_client
net+tls, aggregator) `--strict-effects` — **built**. Rust `dce_tests` (24)
— ok.

Regression-guard: 9-й `test` в `d432_auto_cleanup_hybrid_c.nv` (`"D432:
break inside a nested empty loop must not bleed into an outer auto-cleanup
scope"`) — изолирует дефект от нативного RwLock (локальный ресурс +
`exit_calls`, тот же CAS-retry-loop shape).

## [M-175-realtime-ban-method-call-blind] — D64/D63 suspend-effect scan слеп на instance-method call (Plan 175 Ф.2-v3, 2026-07-22)

**ЧАСТИЧНО ЗАКРЫТО (2026-07-22, Фаза 5 регресс-фикс):** отдельный, СТРУКТУРНО ИДЕНТИЧНЫЙ чекер — Supervisor-хендлер suspend-scan (Q-блок 173.2, `types/mod.rs` `walk_expr_for_handler_lits`, err-код `E_SUPERVISOR_HANDLER_SUSPEND`) — оказался БЛОКИРУЮЩИМ (не «followup, не блокирует», как изначально помечено ниже): `spec_tests/conformance/neg/handler_sleep_neg.nv` дал NEG-NO-ERROR на полном мега-CU. Тот чекер расширен — ЛЮБОЙ `.sleep()`/`.sleep_until()` method-call (не только `Time.sleep(...)`) теперь триггерит `E_SUPERVISOR_HANDLER_SUSPEND`, независимо от receiver'а (эвристика без type-инференции — имена `sleep`/`sleep_until` в std принадлежат только `Duration`/`Monotonic`, узкий контекст оправдывает риск false-positive). **D64/D63-ветка (realtime/forbid, types/mod.rs `check_callee_effects`) остаётся ОТКРЫТОЙ** — см. ниже, не путать с закрытой Supervisor-веткой.

**Найдено при:** ретипизации Time (Ф.2-v3) — метод-канон `d.sleep()` стал promoted idiom вместо `Time.sleep(d)`/free `sleep(d)`.

**Дефект:** `check_expr_forbid`/`check_callee_effects` (`compiler-codegen/src/types/mod.rs`) детектирует suspend-эффект внутри `realtime {}`/`forbid`-блока СИНТАКСИЧЕСКИ — только `Effect.op(...)`-shaped path (`path.len()==2 && effect_decls.contains(path[0])`, ИЛИ qualified free-fn/static-method call через `method_table`). Instance-method call на произвольном expression-receiver (`expr.method()`, напр. `d.sleep()` где `d` — переменная/выражение типа `Duration`) НЕ резолвится — веткa явно помечена "dynamic member-call; не resolve'им" (уже существовавшее ограничение, НЕ введено этой волной). Значит `#realtime fn` вызывающая suspend-effect ТОЛЬКО через метод (`d.sleep()`, не `Time.sleep(d)`) может пройти D64-гард необнаруженной.

**Почему не пофикшено этой же волной:** нужна receiver-type-инференция В ЭТОМ checkpoint'е (какой тип у `d`, чтобы найти `method_table[Тип]["sleep"]`) — checker уже конструирует такую инфраструктуру для IDE (`expr_types`/`resolved_types` side-channel, `record_expr_types`-флаг), но НЕ включена по умолчанию в основной check-pass. Масштаб фикса — architectural (не point-patch), риск регрессии в основном чекере высок при спешке.

**Текущий обход:** негатив-фикстура (`spec_tests/conformance/neg/d316_realtime_sleep_neg.nv`) продолжает звать `Time.sleep(d)` (qualified form, ловится) — обе формы валидны семантически (только `.nv`-сахарные free-функции `sleep`/`sleep_until` ретрактированы, не сам effect-op), так что гейт не ослаблен, просто использует ДРУГОЙ (покрытый) call-shape.

**Follow-up:** расширить D64 (`realtime_suspend_effect`) и D63 (`forbid`) сканы на instance-method-call форму — включить `record_expr_types`-инфраструктуру (или эквивалент) на этом checkpoint'е основного (не только IDE) прохода, резолвить receiver-тип перед `method_table`-lookup.

## [M-175-lazy-const-crossmodule-collision] — module-level `ro NAME` lazy const без module-qualification (Plan 175 Ф.2-v3, 2026-07-22) — CLOSED

**Найдено при:** полном мега-CU регресс-бисекции после Ф.2-v3 (см. amend в spec/decisions/04-effects.md «Фаза 5»), баг НЕ виден на per-file/per-directory точечных гейтах.

**Дефект:** `emit_c.rs`'s Plan 91.12 (D307) module-qualification pre-pass (`private_const_c_names`) обрабатывал только `Item::Const` (`const NAME = …`/`export const …`). Module-level `ro NAME = expr` (`Item::Let`, non-constexpr initializer → lazy-init путь) НИКОГДА не получал qualified C-имя — `emit_lazy_const` всегда эмитил bare `_nova_const_<name>_value`, вне зависимости от коллизий. Безобидно, пока bare-имя `ro`-биндинга нигде больше не встречалось; Ф.1's `std/prelude/effects.nv` import `Duration/Timestamp/Monotonic` сделал `duration/core.nv`'s `export const ZERO/SECOND/MINUTE/HOUR Duration` transitively reachable из КАЖДОГО CU — файл с собственным приватным `ro ZERO`/`ro SECOND` столкнулся с ними на уровне C-символа (repro: `spec_tests/conformance/standalone/repro_const_dup.nv`).

**Fix (`compiler-codegen/src/codegen/emit_c.rs`):**
- `emit_module`'s Step-1 const-grouping pre-pass расширен на `Item::Let` (Pattern-matching ОБЕ формы бинда — `Pattern::Ident` И `Pattern::Variant{kind:Unit}`, т.к. bare ALL-CAPS-имя парсится во вторую форму).
- `emit_lazy_const(name, c_name, ty_c, value)` — новый параметр `c_name` (qualifier для `_nova_const_<c_name>_value`), отдельно от `name` (bare source identifier, всё ещё управляет `lazy_consts`/`var_types`/topo-sort).
- `emit_const_decl`'s lazy (Err) branch и `Item::Let`-call-site теперь передают ПРАВИЛЬНО вычисленный `c_name` (раньше `emit_const_decl` вычислял `c_name`, но тут же ронял его, передавая bare `c.name`).
- REFERENCE-site (`ExprKind::Ident`, lazy branch) теперь ТОЖЕ смотрит `private_const_c_names` для qualifier перед построением `_nova_const_<qualifier>_value`.

**Гейт:** `repro_const_dup.nv` PASS; полный мега-CU (`nova test --positive --compile-error spec_tests/conformance`) — PASS 527 / FAIL 0 / SKIP 55 (совпадает с историческим чистым baseline).

## [M-priv-field-bare-literal-context-infer-bypass] — `priv`/module-priv field-init check не видит bare (unqualified) record-literal с типом из контекста (найдено Plan 175.2 Ф.2-v4 П5, 2026-07-22)

**Найдено при:** П5 (`priv nanos` на `Monotonic`/`Timestamp`/`Duration`, D220 per-field) — репро-гейт владельца («внешний `{nanos:}` → E-ошибка») неожиданно дал **PASS** (не ошибку) для `fn bad() -> Monotonic => { nanos: 42 }` (bare literal, тип берётся из объявленного return-type функции), хотя `nanos` уже `priv`.

**Дефект:** `E_PRIV_FIELD_INIT`/`E_FIELD_MODULE_PRIVATE` (`compiler-codegen/src/types/mod.rs`, `ExprKind::RecordLit` priv-check, ~строка 6604) гейтится на `if let Some(tn) = type_name` — т.е. проверяет ТОЛЬКО литералы с явным именем типа (`TypeName { field: … }`). Bare-литерал (`{ field: … }`, `type_name: None`, тип выводится из контекста — return-position, let-annotation, arg-position и т.п.) НИКОГДА не проходит через этот код-путь, независимо от того, какой тип реально был выведен. Усугубляется тем, что для типов вроде `Duration`/`Timestamp`/`Monotonic` (все три — единственное поле `nanos`) существующий линт «redundant type prefix on record literal» АКТИВНО призывает убирать явное имя типа именно в тех позициях (return-position), где контекст уже его определяет — т.е. канонический/promoted стиль Nova прямо ведёт к форме, которая обходит priv-check.

**Подтверждено как ОБЩИЙ (не Time-специфичный) баг:** репро на СОВЕРШЕННО не связанном, задолго ДО этой волны существующем priv-поле — `std/src/io/console.nv` `export type Stdin { priv unit int }` — `fn bad() -> Stdin => { unit: 42 }` тоже проходит БЕЗ ошибки. Значит это архитектурный пробел в priv-enforcement (затрагивает ЛЮБОЙ record с `priv`/module-priv полем), не что-то, что внесла эта волна — П5 лишь СДЕЛАЛ его наблюдаемым на новом поле.

**Почему не пофикшено этой же волной:** типов context-inference для bare RecordLit НЕСКОЛЬКО отдельных каналов (return-type unify, let-annotation, arg-position, `record_field_names` CU-wide field-set match и др. — `types/mod.rs` содержит минимум 25+ разных `ExprKind::RecordLit` match-сайтов на разных этапах чекера), и НИ ОДИН из них не переиспользует/не проставляет `type_name` обратно в AST до того, как priv-check уже отработал (или отработал бы, будь у него доступ к разрешённому типу). Закрытие требует либо (a) провести priv-check ПОСЛЕ финальной резолюции типа bare-литерала (единая точка, а не 25+ разбросанных мест), либо (b) продублировать priv-gate в каждом канале, который умеет резолвить bare-literal в конкретный record-тип. Масштаб — архитектурный (языковой уровень, не Time-специфичный), риск регрессии в основном чекере высок при спешке; вне объёма Time-API-полировки.

**Текущий обход:** ни одного (defense-in-depth частично держится: explicit `TypeName { field: … }` форма ловится корректно — см. репро `Monotonic { nanos: 42 }` → `E_PRIV_FIELD_INIT`/линт-конфликт; обходится только bare-контекстная форма).

**Follow-up:** унифицировать priv-field-init enforcement в ОДНОМ пост-резолюции чекпоинте (после того, как bare RecordLit получил свой финальный `ResolvedType`, независимо от канала-источника), либо явно опросить каждый существующий канал context-inference и продублировать priv-gate. Затронутые типы прямо сейчас в std: `Duration`/`Timestamp`/`Monotonic` (`time.duration`), `Stdin`/`Stdout`/`Stderr` (`std.io`) — возможно другие, не проверено полным грепом.

## [M-consume-fn-value-call-arg-not-tracked] — ✅ CLOSED 2026-07-24 (окно 67 consume-звучность, №55 221.1 registry)

**Находка (2026-07-23, финальная сантехника):** consume-значение, переданное через вызов ПЕРВОКЛАССНОГО fn-значения (`f(r)`, статический тип `f` — голый `fn(T) -> U`), не распознавалось D131/D133 как consumed — user-типы БЕЗ `@cleanup`-фолбэка ловили ложноположительный `D133-not-consumed` на exit'е объемлющего scope'а; типы С `@cleanup` (реальный workaround — nova-http `ws/socket.nv`) маскировали это (auto-cleanup-eligible типы не ошибаются на scope-exit).

**Корень — language-level, не только checker-пробел:** грамматика `fn(T) -> U` НЕ несёт per-param `consume`-квалификатор вообще (`parse_fn_type_signature`, `compiler-codegen/src/parser/mod.rs`, парсит параметр как голый `parse_type()`); иллюстративный `fn(consume T) -> U` из D156 HOF-раздела (`spec/decisions/02-types.md`) — аспирационный design sketch, никогда не был проведён в конкретный (non-generic) fn-type parser. Чекер категорически не имеет статической consume/view-сигнатуры для вызова через fn-значение.

**Фикс (checker-эвристика, БЕЗ новой грамматики — амендмент D131/D133 в `spec/decisions/05-memory.md`):** когда callee вызова — `Ident`, НЕ резолвящийся в зарегистрированную top-level `fn` (пустой `consume_idxs`) и не consume-closure, но являющийся известным ЛОКАЛЬНЫМ биндингом (параметр/`let`/alias) — bare-Ident consume-обязательный аргумент трактуется как потреблённый этим вызовом (`compiler-codegen/src/types/mod.rs`, `ExprKind::Call`'s `ExprKind::Ident(fname)`-ветка). Соответствует уже существующей "default = silent-ignore" backward-compat политике (D156). Побочный эффект — реальное улучшение звучности: `f(r); r.close()` (двойной close через fn-значение) раньше молча проходил, теперь корректно триггерит `D131`.

**Гейты:** `consume_fn_value_call_arg_ok.nv` (pos) + `neg/consume_fn_value_call_arg_double_close_neg.nv` (D131 double-close теперь ловится) — оба зелёные; regression — 11 существующих d157/d180/detach/M-176 фикстур без изменений в вердиктах; targeted std/net(7) + std/fs(3) + std/concurrency(9) + доп. fn-value-bearing файлы (sync.nv/effects.nv/sql.nv/orm_demo.nv) — все GREEN; flagship aggregator `--strict-effects` GREEN.

**Честный defer (followup ниже):** эвристика НЕ отличает "callee реально consume'ит" от "callee лишь читает view-style" — оба трактуются как consumed (тот же trade-off, что уже принят для generic HOF без `[T consume]` bound). Полное решение = language-расширение, см. `[M-fn-type-consume-param-syntax]` ниже.

## [M-fn-type-consume-param-syntax] — followup из [M-consume-fn-value-call-arg-not-tracked] (2026-07-24): `consume`-квалификатор в конкретных fn-type позициях

**Суть:** `fn(T) -> U` (конкретный, non-generic fn-type — параметры, переменные, поля) не может статически заявить "этот параметр consume'ит своё значение" — `parse_fn_type_signature` не принимает `consume` перед типом параметра (в отличие от `fn name(consume x T)`-деклараций, где это работает). D156 (`spec/decisions/02-types.md`) уже иллюстрирует желаемый синтаксис `fn(consume T) -> U` для generic-HOF bound-контекста, но это НЕ проведено в парсер для конкретных fn-type аннотаций.

**Объём полного решения:** (1) parser — `parse_fn_type_signature`, per-param optional `consume` prefix, аналогично generic-decl `consume`-suffix (`parse_type_args`); (2) type-compatibility — presented-fn/closure/named-fn, назначаемый в `fn(consume T) -> U`-типизированную позицию, обязан сам иметь `consume` на соответствующем параметре (иначе type-mismatch, не просто consume-checker warning); (3) consume-checker — `ExprKind::Call`'s `Ident`-ветка должна консультировать ЭТУ типовую информацию (а не эвристику "любой локальный биндинг = consume", которую применил checker-фикс выше) для ТОЧНОГО (не эвристического) tracking; (4) ABI/mangling-последствия для fn-value passing — не аудировано.

**Почему не в этом окне:** language-меняющая работа (новая грамматика + type-compat + возможные ABI-последствия) — отдельный design-цикл с собственным D-амендментом и владельческим решением, не bundled в checker-фикс окна 67. Текущая эвристика (см. закрытый маркер выше) — sound-improving, но НЕ полная замена.

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-fn-type-consume-param-syntax]` | Concrete `fn(T) -> U` (non-generic fn-type) не несёт `consume`-квалификатор на параметрах — D156's `fn(consume T) -> U` HOF-иллюстрация никогда не была проведена в парсер для конкретных (не generic-bound) fn-type позиций. Полная замена checker-эвристике из `[M-consume-fn-value-call-arg-not-tracked]` требует parser + type-compat + ABI-аудит. | consume-checker / parser | P2 |

## 222.3 §5 (extractor arity-overload sugar) — ДВА новых блокера найдены окном p2223 (2026-07-26), НЕ ЗАКРЫТЫ, сахар РАЗВЁРНУТ обратно

**Контекст:** окно p2223 (sonnet) пришло с решением владельца «блокеры №34/№105 сняты» и попыталось реализовать плана-222.3 §5 арность-сахар (`Router.@get[T1 FromRequest, R IntoResponse](path, h fn(T1) -> R)` и арность 2/3) в `nova-polaris`. №34/№105 ДЕЙСТВИТЕЛЬНО закрыты для СВОИХ узких форм (см. записи №34/№105 выше в 221.1-bug-sweep.md), но добавление РЕАЛЬНОГО сахара тут же уткнулось в ДВА НОВЫХ, ГЛУБЖЕ лежащих класса — оба вскрыты минимальными изолированными репро (не полагаясь на nova-http/polaris), оба НЕ закрыты, сахар из `nova-polaris` окно РАЗВЕРНУЛО обратно (весь `.nv`-код + доки + тесты — байт-идентичный откат, `git diff` пуст).

**Блокер А — `[M-2223-closurefull-generic-overload-resolution]` (checker+codegen, частично прототипирован, ОТКАЧЕН).** D84 «конкретный бьёт generic»-тайбрейк (types/mod.rs `check_instance_overload`, ~12509) и его codegen-зеркало (`emit_c.rs` ~41158, `has_bare_closure_arg`-гейт) ИСКЛЮЧАЮТ closure-литералы из тайбрейка целиком — комментарий объясняет почему для `ClosureLight` (нетипизированной `|x| …`, return-type leniency у `assignable`), но `ClosureFull` (`fn(x T) -> U { … }`, ПОЛНОСТЬЮ типизированная по грамматике, БЕЗ этой leniency) исключена туда же по недосмотру. Реальные вызовы Router.@get в polaris — ВСЕГДА `ClosureFull` (канон `[M-closure-light-newtype-over-fn-param-misinfer]`), поэтому: (1) добавление ОДНОГО generic-сиблинга с тем же именем ломает codegen существующих конкретных вызовов (E7001, "closure-arg return type (R)") — тот же класс, что и №34, но на ClosureFull, не на bare-closure/Call-expr; (2) `overload_applicability` (types/mod.rs ~12116) ТОЖЕ не различает ДВА generic-сиблинга разной арности (`fn(T1)->R` vs `fn(T1,T2)->R`) для одного ClosureFull-аргумента — `assignable` схлопывает ЛЮБОЙ `TypeRef::Func`-ожидаемый тип в `ResolvedType::Any` (permissive-дизайн «codegen делает финальный выбор»), так что ОБА сиблинга регистрируются «совместимыми» → `compat_spans.len()>1` → `resolved_callees` пуст → codegen's single-valued `mono_method_decls` (`HashMap<(String,String), FnDecl>`, «последняя регистрация побеждает») подставляет ЧУЖОЕ тело — silent wrong-type dispatch (`int.from_request(...)`-класс ошибок, не всегда даже честный CC-FAIL). Окно спроектировало и ЛОКАЛЬНО ПРОВЕРИЛО (минимальные репро в scratchpad, не сохранены) послойный фикс — (а) `closure_args_match_concrete`/`peel_func_shape_depth` (новые методы в `BoundCtx`, types/mod.rs, рядом с `concrete_sibling_return_type_ok`): для `ClosureFull`-аргумента требовать ТОЧНОЕ структурное совпадение (`typeref_equal`) с ПЕРЕОБВЁРНУТЫМ (через fn-newtype) Func-типом конкретного кандидата, вместо permissive-коллапса; (б) зеркальный гейт в `emit_c.rs` (~41158) — доверять `resolved_callees` для `ClosureFull`, не только для не-closure арг.; (в) `mono_method_decls_by_span: HashMap<Span, FnDecl>` (новое поле emit_c.rs, рядом с `mono_method_decls`) — точный lookup по `resolved_callees`-span вместо single-valued карты; (г) `overload_applicability`-гейт — арность+точный-тип-check для ПОЛНОСТЬЮ-конкретных Func-параметров (не упоминающих `callee_gs`), permissive (только арность) для по-настоящему generic-параметров. **(а)+(б)+(в) верифицированы вместе на изолированных репро** (concrete+1 generic sibling, конкурирующие вызовы ОБОИХ путей — PASS, включая рантайм). **(г) — РЕГРЕССИЯ**: делает 2-generic-siblings-разной-арности случай компилируемым НА ИЗОЛИРОВАННЫХ РЕПРО, но **ломает `examples/flagship/aggregator --strict-effects`** (не-Router, не-polaris код — `HandlersDto`/`JsonSerializer`-моно, "no member named 'state'/'error' in struct NovaValue_HandlersDto" — CC-FAIL) — т.е. (г) задевает КАКОЙ-ТО ДРУГОЙ существующий generic-overload-паттерн где-то в std/флагмане, не изолированный и не диагностированный этим окном (CPU/время окна исчерпаны на диагностике; корень (г)-регрессии НЕ найден). Все 4 части ОТКАЧЕНЫ (`git show HEAD:… > file` на оба файла, подтверждено `git diff` = 0 строк, `wc -l emit_c.rs` = ratchet-baseline 63096, mega-CU `spec_tests/conformance` **586/0/67 ДО отката** тоже зелёный — регрессия (г) НЕ ловится conformance, только флагманом, что подтверждает правило CLAUDE.md «поведенческие слияния обязаны собирать флагман, conformance app-регрессии не ловит»).

**Блокер Б — `[M-2223-generic-method-instance-mono-symbol-collision]` (глубже, НЕ прототипирован, вероятно требует архитектурной работы над mono-фазой).** Обнаружен ПОСЛЕ блокера А (когда А частично заработал на изолированном репро) — **более фундаментальный**: ОДИН declared method-level-generic instance-метод (`fn Router mut @get[T1 FromRequest, R IntoResponse](h fn(T1) -> R)`, БЕЗ каких-либо сиблингов/перегрузок вообще), вызванный ДВАЖДЫ с РАЗНЫМИ конкретными типами (`r1.get(fn(a ExA)->Resp{…})` затем `r1b.get(fn(b ExB)->Resp{…})`, разные `Router`-инстансы) — CC-FAIL: `passing 'NovaValue_ExB' to parameter of incompatible type 'NovaValue_ExA'`. Т.е. ВТОРОЙ вызов линкуется на C-СИМВОЛ/ТЕЛО ПЕРВОГО. Корень (гипотеза, не подтверждена глубже): `mono_method_decls`/`mono_method_decls_by_span`-стиль per-(receiver-type, method-name) регистрация мангли́т C-имя БЕЗ учёта фактических type-args инстанциации (в отличие от receiver-generic моно, `Vec[T]`, где T — часть receiver-mangling и потому различается естественно) — метод-level-generic (typevar живёт ТОЛЬКО в параметрах/возврате метода, не в receiver) не получает per-инстанс мангл вовсе, т.е. «mono как побочный эффект кодогена конкретного call-site», а не «mono как фаза сбора всех инстанциаций» (rustc-эталон, см. `feedback-rustc-as-reference.md`) — для receiver-independent generics эта фаза, похоже, никогда и не строилась. **Это блокирует ЛЮБОЕ реалистичное использование §5-сахара** (Router с ДВУМЯ и более route'ами через разные extractor-типы — нормальный кейс любого реального приложения) **даже если Блокер А будет закрыт без регрессий**. НЕ исследовано глубже (не диагностирован конкретный код-путь мангла) — требует отдельного design-окна, вероятно язык компилятор-канала уровня «mono как фаза», не точечный checker-патч. | codegen mono-instantiation (method-level generics) + checker overload-resolution | **P1-P2 — БЛОКИРУЕТ 222.3 §5 (арность-сахар extractors); Блокер Б, вероятно, БЛОКИРУЕТ и любые ДРУГИЕ будущие method-level-generic API той же формы (не специфично для http/polaris)** |

**Вывод для 222.3:** §5 (арность-сахар регистрации) остаётся НЕРЕАЛИЗОВАННЫМ. `nova-polaris` (worktree `nova-polaris-pext`, ветка `p-222-3-sugar`) полностью откачен к состоянию до окна (byte-identical, `git status`/`git diff` чисты) — экстракторы (Path/Query/Json/Bytes/Text/Headers, `extract.nv`, УЖЕ реализованные волной до этого окна) не тронуты и продолжают работать через низкоуровневую форму (`Router.@get(path, Handler)` + ручной `T.from_request(req)`), задокументированную в `docs/handlers-response.md`/`roadmap.md` как today's canon — обе доки НЕ изменены (ложного «done»-статуса нет). Хвост №105 (Call-выражение, возвращающее голое fn-значение) НЕ проверен этим окном — Блокер А/Б встали раньше в очереди приёмочных кейсов.

## 222.3 §5, ПОВТОР (окно p2223b, sonnet, 2026-07-26) — №124/125/127/128/130 подтверждены закрытыми (`d69d5e49e`), но ТРЕТИЙ новый блокер найден после реализации; сахар СНОВА развёрнут обратно

**Контекст:** окно пришло с решением интегратора «№124/125/127/128/130 закрыты, путь открыт» (проба форм компилировалась и бежала в `main`). Реализовало арность-сахар (1-3 экстрактора, все 5 глаголов) в `nova-polaris` (worktree `nova-polaris-pext2`, ветка `p-222-3-sugar-retry`). `nova test src --strict-effects` на ПОЛНОМ пакете (все 5×3=15 деклараций + новые тесты одновременно) вскрыл РОВНО ОДИН НОВЫЙ компилятор-гэп (закрыт синхронно в этом же окне), затем ЕЩЁ ОДИН — глубже — НЕ закрыт.

**Гэп №1 — `[M-closurefull-arity-multiparam-typevar-return-infer]` — ✅ ЗАКРЫТ этим окном (nova-p2223b, emit_c.rs ~41372).** `resolve_method_return_with_closure_args` (checker, types/mod.rs — единственный производитель `node_substs` для closure-arg-return-инференса) архитектурно гейтится на резолв ВНЕШНЕГО return-типа callee (см. его же `if method_names.is_empty() { return None }` комментарий «покрыто базовой resolve_instance_method_return») — НЕ вызывается вовсе, когда внешний return уже конкретен (`Result[Router, HttpError]`, как у ВСЕХ §5-сигнатур — R живёт ТОЛЬКО внутри closure-параметра, не в возврате метода). Канал для этого класса вызовов НЕДОСТИЖИМ без более широкой перестройки producer'а (когда/зачем его вызывать). Легаси-фоллбек (emit_c.rs, `closure_ret_c` match ~41326) — единственный оставшийся путь — имел арм ТОЛЬКО для `ClosureLight`; `ClosureFull` (канон-форма §5, D52/D55) падал в `_ => String::new()`, оставляя R (и T1/T2, если 2+) непривязанными → честный `[E7001]` через `err_no_int_fallback`, НЕ silent miscompile. Фикс: симметричный `ClosureFull`-арм — биндит callee-typevars из литерала СВОИХ явных param-типов (`sb.params[i].ty`) напрямую (без body-walk, т.к. грамматика ClosureFull ВСЕГДА их даёт), читает return-C-type с `sb.return_type` (всегда `Some`). Верифицировано: 4 изолированных репро (арность 1/2/3, с/без конкурирующего конкретного sibling) — все PASS; мега-CU `spec_tests/conformance` **586/1/67** (единственный FAIL — документированный pre-existing intermittent `a_q3_println_debug_record`, воспроизведён и БЕЗ диффа на этой же машине) — 0 регрессий; `check std/src` байт-канон 142/27/1040 не сдвинулся; флагман `--strict-effects` + smoke (curl `/api/snapshot`) зелёные. `arch-ratchet.baseline` lines 63096→63104 (осознанный сдвиг, обоснование в самом файле — эмит-слой закрывает РЕАЛЬНЫЙ семантический гэп, симметричный уже существовавшему `ClosureLight`-арму, не легаси-раздутие). **Коммит: `nova-p2223b`, ветка `p222-3-retry`, checkpoint `53eb2c575`** (интегратору предстоит смёржить/перенести в main).

**Гэп №2 — `[M-2223-arity-sibling-static-protocol-dispatch-int-fallback]` — НЕ ЗАКРЫТ, НОВЫЙ, глубже.** После закрытия Гэпа №1 полный `nova test src --strict-effects` на пакете дал НОВУЮ ошибку: `CODEGEN-FAIL ... [E_UNKNOWN_STATIC_METHOD] int.from_request(...) — у примитива int нет статического метода from_request` — распространившуюся на ВСЕ ~39 целей пакета (общая C-библиотека/CU, как и в Блокере А прошлого окна — одна ошибка ломает всех потребителей той же сборки). Диагностировано минимальным ИЗОЛИРОВАННЫМ (вне polaris/http) репро: ДВА (или больше) method-level-generic сиблинга ОДНОГО имени (`@get[T1,R](x, h fn(T1)->R)` и `@get[T1,T2,R](x, h fn(T1,T2)->R)`), КАЖДЫЙ из которых внутри СВОЕГО тела статически зовёт протокольный метод на СВОЁМ method-level typevar-параметре (`T1.from_req(req)` — ТОЧНАЯ форма §5-сахара, `T1.from_request(req)`) — компилируется НЕВЕРНО: T1 в одном из сиблингов падает в стандартный «unresolved generic → nova_int» fallback, затем codegen пытается эмитить static-dispatch на `int`. Подтверждено: (а) РАЗМЕР сиблингов не важен — 2 сиблинга уже достаточно (не только 3); (б) МНОЖЕСТВЕННОСТЬ конкретных implementers протокола САМА ПО СЕБЕ не виновата — репро с 5 concrete-implementers протокола и ОДНИМ (arity-1-only, без сиблингов) generic-методом компилируется и бежит ЧИСТО (т.е. T1-substitution → static-dispatch per-instantiation в принципе работает корректно, когда сиблингов нет); (в) СПЕЦИФИЧНО присутствие ≥2 ОДНОИМЁННЫХ arity-siblings, каждый статически диспатчащий на СВОЙ typevar, триггерит проблему — тот же общий класс, что и №125 (`method_receivers["clone"]` single-key last-wins), но, судя по всему, ДРУГОЙ, ещё не проаудированный single-key registry (используемый codegen'ом при резолве static-method-call на method-level-generic receiver, возможно смежный с `mono_method_decls`/`method_overloads`, НЕ идентифицирован конкретный код-путь — бюджет окна исчерпан на изоляции симптома, не хватило на трассировку конкретной регистрации). Минимальный репро (`scratchpad/repro105k.nv`, вне пакета, самодостаточный, ~50 строк) сохранён ТОЛЬКО локально в scratchpad агента этого окна — НЕ закоммичен как conformance-фикстура (нет времени завести её по конвенции); интегратору/следующему окну нужно будет пересоздать по описанию выше (структура: `R105.new()`, protocol `FromReq105` с `.from_req(req int) -> Result[Self, str]`, ДВА impl-типа `A105`/`B105`, `R105 mut @get(x int, h Handler105) -> R105` (конкретный, arity 2 outer: x,h), затем ДВА generic-сиблинга `@get[T1 FromReq105, R](x int, h fn(T1)->R)` и `@get[T1,T2,R](x int, h fn(T1,T2)->R)`, ОБА делегирующие в конкретный `@get(x, fn(req int)->str{ match T1.from_req(req) {...} })` — arity-2 body ДОПОЛНИТЕЛЬНО зовёт `T2.from_req(req)` тем же паттерном; ДВА раздельных вызова `r.get(1, fn(a A105)->str{...})` и `r.get(2, fn(a A105,b B105)->str{...})` в одном тесте). **Это блокирует ЛЮБУЮ комбинацию 2+ extractors на одном хендлере через §5-сахар** — самый ценный кейс сахара (Axum-паритет: `fn handler(Path(id): Path<T>, Query(q): Query<U>)`). **Решение окна**: НЕ рисковать очередным полу-фиксом (прецедент прошлого окна — попытка (г) сломала флагман) — §5-сахар СНОВА развёрнут байт-идентично (см. ниже), ТОЛЬКО Гэп №1 (безопасный, верифицированный, эмит-слой) оставлен в компиляторе.

**Итог окна p2223b:** `nova-polaris-pext2` (ветка `p-222-3-sugar-retry`) — `src/extract.nv`/`src/extract_test.nv` откачены байт-идентично к `56a2bc9` (`diff <(git show 56a2bc9:FILE) FILE` = 0 строк для обоих) — §5 остаётся НЕ реализованным, банер-комментарий «BLOCKED, STOP+REPORT» + низкоуровневая форма — единственный рабочий путь, как и раньше; доки (`handlers-response.md`/`roadmap.md`) НЕ трогались (уже честно говорят «planned»). `nova-p2223b` (ветка `p222-3-retry`) — Гэп №1 (emit_c.rs ClosureFull-арм) ОСТАВЛЕН, checkpoint-коммит `53eb2c575`; `arch-ratchet.baseline` обновлён (63096→63104, обоснование в файле). Гейты обоих репо зелёные: nova — мега-CU 586/1/67 (известный флейк), `check std` 142/27/1040, arch-ratchet ok, флагман+smoke ok; polaris — `test src --strict-effects` 33/0/16 (канон, ПОСЛЕ отката сахара), `run_smokes.sh` 10/10.

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-closurefull-arity-multiparam-typevar-return-infer]` | `ClosureFull`-литерал закрывающий closure-arg-return-инференс (emit_c.rs) не имел арма вовсе — только `ClosureLight`; закрыт этим окном (симметричный арм, ~41372) | codegen closure-arg return inference | ✅ ЗАКРЫТ (nova-p2223b, checkpoint `53eb2c575`, интегратору смёржить) |
| `[M-2223-arity-sibling-static-protocol-dispatch-int-fallback]` | 2+ method-level-generic сиблинга ОДНОГО имени, каждый статически зовущий протокольный метод на СВОЁМ typevar (§5-сахара точная форма) → T1 в одном из тел падает в nova_int fallback → `[E_UNKNOWN_STATIC_METHOD] int.<method>(...)`. Repro вне пакета в тексте выше (не закоммичен как фикстура) | codegen static-method-dispatch-on-generic-typevar (вероятно смежный single-key registry классу №125) | 🔴 P1 — БЛОКИРУЕТ 222.3 §5 многоэкстракторный кейс (самый ценный); нужно отдельное design/диагностика-окно |

**ОКНО p124 (2026-07-26, sonnet) — №124 ЗАКРЫТ (сужен, безопасное подмн-во), №125 ЧАСТИЧНО ЗАКРЫТ (гипотеза опровергнута, РЕАЛЬНЫЙ root cause найден и исправлен в mega-CU-репро) + НОВАЯ находка (static-generic crash).**

**№124 — `[M-2223-closurefull-generic-overload-resolution]` ✅ ЗАКРЫТА безопасная часть (а)+(б), (г) НЕ ВОСПРОИЗВЕДЕНА ЗАНОВО (сознательно не тронута).** Реализованы ТОЛЬКО (а)+(б) из прошлого окна: (а) `closure_args_match_concrete`/`peel_func_shape_depth` (types/mod.rs, рядом с `concrete_sibling_return_type_ok`) — для `ClosureFull`-аргумента требуют ТОЧНОЕ структурное совпадение (`typeref_equal`) закрытого-литерала-сигнатуры с Func-параметром конкретного кандидата; D84-тайбрейк сужен с `has_bare_closure_arg` (ClosureLight ИЛИ ClosureFull) до `has_bare_closurelight_arg` (ТОЛЬКО ClosureLight) — `ClosureFull` больше не exempt целиком, а проходит через новую точную проверку. (б) зеркальный гейт в `emit_c.rs` (~41163, тот же rename `has_bare_closurelight_arg`) — доверяет `resolved_callees` для `ClosureFull` (не только для Call-expr арг.), т.к. канал теперь пишет concrete-span ТОЛЬКО при точном совпадении. Верифицировано: минимальный репро `arity_overload_concrete_vs_bound_generic_closurefull.nv` (concrete `@get` + generic-сиблинг, `ClosureFull`-аргумент — раньше падал `[E7001]`) — PASS; существующие №34/№105 регрессы (`arity_overload_concrete_vs_bound_generic.nv`, `generic_mono_concrete_sibling_multi_r.nv`, `neg/generic_mono_concrete_sibling_named_fn_neg.nv`) — все PASS без изменений; `examples/flagship/aggregator --strict-effects` — собирается ЧИСТО + smoke (`curl /api/snapshot` — валидный JSON с `HandlersDto`/JsonSerializer-полями, процесс поднялся-ответил-погашен) — **(г)-регрессия НЕ повторилась**, т.к. (г) вообще не тронута. **(г) (`overload_applicability`-гейт для 2-generic-сиблингов-разной-арности) сознательно НЕ реализована в этом окне** — задание требовало разобрать корень прошлой регрессии ПЕРЕД реализацией, но root-cause прошлой (г)-регрессии в HandlersDto/JsonSerializer-mono не найден (не хватило бюджета времени после (а)/(б)/№125-расследования); поскольку (г) нужна ТОЛЬКО для generic-vs-generic-разной-арности (более узкий, более редкий кейс, чем concrete-vs-generic, который (а)+(б) уже закрывают), решено оставить (г) как отдельную, отдельно риск-профилированную будущую работу, а не рисковать повтором регрессии blind. **Побочная находка (независима от №124/№125, НЕ в объёме этого окна):** generic-сиблинга СОБСТВЕННЫЙ `ClosureFull`-вызов (`f.get(path, fn(n int)->int{...})` на generic `@get[R](path, f fn(int)->R)`, БЕЗ concrete-конфликта) — уже ПРЕДСУЩЕСТВУЮЩИ ломается `[E7001]` НА БАЗОВОМ (немодифицированном) компиляторе, подтверждено `git show HEAD:… > file` + пересборка + repro. ClosureFull-arg return-type-инференс в generic-mono ветке (`~41305+`, `ret_slot_name`/`closure_return_generics`) не находит R для полностью-типизированного литерала — отдельный, НЕ связанный с D84-тайбрейком гэп (bare `ClosureLight` для ЭТОЙ же формы работает, `generic_mono_concrete_sibling_multi_r.nv`). Новый маркер `[M-closurefull-own-generic-sibling-return-infer-gap]`.

**№125 — `[M-2223-generic-method-instance-mono-symbol-collision]` — ✅ ЧАСТИЧНО ЗАКРЫТА: исходная гипотеза ОПРОВЕРГНУТА, РЕАЛЬНЫЙ root cause найден в mega-CU и исправлен.** Аудит `register_mono_method_instance`/`compute_mono_name` (emit_c.rs) показал: per-инстанс mono ДЕЙСТВИТЕЛЬНО ключуется по ПОЛНОМУ `type_subst` конкретного call-site (`Nova_<Recv>_method_<name>____<arg1>__<arg2>`), НЕ по (receiver, method) одному — вопреки гипотезе прошлого окна. ~10 структурных вариантов минимального ИЗОЛИРОВАННОГО (single-file) репро (bare scalar T, user record T, `value`-kind (`NovaValue_`) T, protocol-BOUND T, T только внутри closure-типизированного параметра — точный диагностированный вид `h fn(T1) -> R`, `-> @` self-chain-возврат, раздельные receiver-инстансы/`test{}`-блоки, 2 и 3 инстанциации) — НИ ОДИН не воспроизвёл коллизию в изоляции. **Ключевая находка**: запустив ОДИН из этих репро-файлов (`x.clone()` внутри `@m[T](x T)`, `T` — `value`-kind) КАК ЧАСТЬ mega-CU `spec_tests/conformance` (folder=module — файл автоматически тянет ВСЕ ~600 co-equal файлов каталога в ОДИН compile-unit), коллизия ПРОЯВИЛАСЬ: CC-FAIL `returning 'Nova_D230Point*' from a function with incompatible result type 'NovaValue_ExA125z'` — тело generic-метода для T=ExA125z буквально звало ЧУЖОЙ, никак не связанный `D230PointZ.@clone()` (из СОВСЕМ другого .nv-файла того же каталога). Минимизировано до ЧИСТОГО 2-файлового репро вне spec_tests (только Router+`@m[T]`+`x.clone()` в одном файле, `#impl(Clone)`-heap-тип в другом, ОБА в одном module-каталоге). Root cause (НЕ в `register_mono_method_instance`): `x.clone()` на BARE-generic-typed ЗНАЧЕНИИ дispatch'ится через Plan-138.4-Ф.1-G-C identity-copy guard (emit_c.rs ~40407) — тот guard проверяет ТОЛЬКО heap-форму (`obj_ty.starts_with("Nova_") && obj_ty.ends_with('*')`), пропуская value-kind receiver (`NovaValue_X`, БЕЗ trailing `*`) — для value-типов guard молча пропускается, попадая в single-key `method_receivers["clone"]` last-wins fallback (ТОТ ЖЕ класс дыры, что G-C изначально закрыл для heap-типов, [M-138.3-clone-bound-unsupported]) — коллидируя с ЛЮБЫМ несвязанным `#impl(Clone)`-типом, зарегистрированным позже в том же compile unit. **Фикс**: guard расширен на value-форму (`obj_ty.starts_with("NovaValue_") && !obj_ty.ends_with('*')`) — идентичная identity-copy семантика. Верификация: 2-файловый мин-репро RED→GREEN; `generic_method_instance_mono_no_sibling.nv` (сначала CC-FAIL В MEGA-CU, PASS после фикса, standalone ВСЕГДА был PASS — ловится ТОЛЬКО mega-CU, см. `feedback-isolate-conformance-before-push.md`); ratchet, №34/№105/#124-регрессы, `check std`, флагман+smoke — все зелёные (см. ГЕЙТ ниже). **НЕ полностью закрыта**: (а) статик-generic форма — см. НОВУЮ находку ниже (отдельный маркер, НЕ исправлена); (б) не исключено, что ДРУГИЕ auto-derive/blanket методы (Equal/Ord/Hash/Display/Debug — не только Clone) используют ТОТ ЖЕ single-key `method_receivers[method_name]` fallback с ТЕМ ЖЕ value-kind гэпом — НЕ проаудировано полностью в это окно (бюджет времени), проверить отдельно.

**НОВАЯ находка (той же окном, побочный продукт аудита mono-фазы) — `[M-static-generic-method-path-call-p67-panic]`.** STATIC (не instance) method-level-generic вызов через Path-форму (`Type.method(x)`, метод `fn Type @method[T](x T) -> T`, receiver сам НЕ generic) КРАШИТ компилятор безусловно — `[P67-LEGACY] Path call return type unknown for method=<name>` (и С explicit turbofish `Type.method[Concrete](x)`, и без). Root-cause (диагностировано, НЕ исправлено): `infer_expr_c_type`'s Channel 2 (`self.resolved_types.get(&expr.id)`, ~58510) — единственный канал, способный корректно вернуть per-call-site substituted return type для generic-возврата — не содержит записи для ЭТОЙ формы вызова (чекер не аннотирует `resolved_types[call.id]` для static-Path-формы с method-own generics и БЕЗ receiver-generics/`Vec`-спецкейса); падение происходит в легаси-фоллбеке `infer_call_ret_c` (~58308), который принципиально НЕ может нести per-instantiation тип (единственная функция на ВСЮ декларацию). Фикс — ТОЛЬКО чекер-канал (§0-доктрина): чекер должен писать `resolved_types[call.id]` = подставленный `R`/`T` для static-generic-Path-вызовов, аналогично тому как это уже работает для instance-формы (`r.m(x)`). НЕ исправлено в этом окне (недостаточно бюджета для безопасного чекер-фикса + верификации без регрессий; emit_c.rs ratchet у потолка — фикс НЕ должен расти легаси-emit). Мин-репро: `type RouterS { tag int } \n export fn RouterS @make[T](x T) -> T { x.clone() } \n test { ro a = RouterS.make(5) }` — падает ДАЖЕ БЕЗ concrete-сиблинга.

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-closurefull-own-generic-sibling-return-infer-gap]` | ПРЕДСУЩЕСТВУЮЩИЙ (не regressed этим окном): полностью-типизированный `ClosureFull` (`fn(x T)->U{...}`) аргумент к МЕТОД-LEVEL-GENERIC методу (без concrete-сиблинга вовсе, ЛИБО с ним — не важно) не может вывести R codegen'ом (`~41305+`, `ret_slot_name`/`closure_return_generics`) — честный `[E7001]`, не миксомпил, но блокирует ClosureFull как форму для generic HOF/Router-сахара целиком. Bare `ClosureLight` для ТОЙ ЖЕ формы работает (`generic_mono_concrete_sibling_multi_r.nv`). | codegen generic-mono closure-arg return infer | 🟡 P2 — блокирует 222.3-сахар (ClosureFull — канон формы, №104) даже после №124 |
| `[M-static-generic-method-path-call-p67-panic]` | STATIC method-level-generic вызов (`Type.method[T](x)`/`Type.method(x)`, receiver не generic) КРАШИТ `[P67-LEGACY]` безусловно — чекер не пишет `resolved_types[call.id]` для этой формы (Channel 2 miss → легаси-fallback panic, `infer_call_ret_c` структурно не может нести per-instantiation тип). Мин-репро в тексте выше. | checker channel (`resolved_types`) для static-Path generic-вызовов | 🔴 P1-P2 — компилятор-окно (mono-фаза №125-трек; static-форма ПОЛНОСТЬЮ нерабочая, не просто гипотетическая коллизия) |

## P3 — Неучтённые маркеры UNREGISTERED (долг 221.1 №155/№161)

> Маркеры из `UNREGISTERED.txt`, найденные при инвентаризации 2026-07-30. Большинство — уже выполненные фиксы/реализации, не зарегистрированные в реестре, либо осознанные ограничения / deferred follow-ups. Заводятся постфактум для закрытия долга храповика.

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-100.6-consume-rvalue-in-result-ok]` | `Result[_,_]` `Ok(consume_expr)` при consume-rvalue (напр. `Ok(String.from("x"))`) — `E_MOVE_IN_RVALUE` (D133 strict-check запрещает перемещение во временный). Рабочий обход: прямой cross-package dispatch без `Ok`-обёртки. Ограничение лексического анализа, не soundness. | floating (checker) | P3 |
| `[M-108-empty-frompairs-nonhashmap-kv-infer-gap]` | ✅ **CLOSED 2026-07-10.** Parser `extract_hashmap_kv` был захардкожен на имя "HashMap" — `empty.fromPairs(...)` на не-HashMap типе не выводил K/V. Фикс: обобщён на любой тип с двумя type-параметрами. | floating (parser) | ✅ DONE |
| `[M-110.9.2-with-exit-timeout-level1]` | Plan 110.9.2 V1.1: `with_exit` timeout Level 1 — timeout-защита эффект-блоков. Реализована в составе Plan 110.9. | Plan 110.9.2 | ✅ DONE |
| `[M-110.9.3-register-finalizer-lifo]` | Plan 110.9.3 V1.1: `register_finalizer` в LIFO-порядке (runtime). Реализована в составе Plan 110.9. | Plan 110.9.3 | ✅ DONE |
| `[M-110.9.4-ffi-cancel-unsafe-lint]` | Plan 110.9.4 V1.1: `W_FFI_CANCEL_UNSAFE` lint — предупреждение при FFI-вызове с cancel-эффектом без `unsafe`. Реализована в составе Plan 110.9. | Plan 110.9.4 | ✅ DONE |
| `[M-110.9.5-on-exit-strict-signature]` | Plan 110.9.5 V1.1: cleanup-функции `on_exit` с non-`Unit` возвратом отвергаются. Реализована в составе Plan 110.9. | Plan 110.9.5 | ✅ DONE |
| `[M-140-generic-method-contract-mono-drop]` | ✅ **FIXED 2026-07-?**. Контракты (`requires`/`ensures`) generic-методов дропались при мономорфизации — условие не эмитилось в mono'd теле. Фикс: contract-clause propagation через mono-кэш. | Plan 140 codegen | ✅ DONE |
| `[M-153.2-collect-into]` | Stage 4 `collect_into` — drain итератора в caller-provided буфер (амортизация alloc). Реализован поверх слот-архитектуры Plan 153.2. | Plan 153.2 | ✅ DONE |
| `[M-156-bare-unit-variant-eq-invalid-cast]` | ✅ **FIXED.** CC-FAIL при `==` между sum-value и bare unit variant — `member reference type 'nova_int' is not a pointer`. Корень: Eq-лоуэринг для bare unit variant (`SomeEnum.Variant == SomeEnum.Variant`) кастовал variant-id как field-pointer. Фикс: emit bare unit variant Eq через variant-id comparison (int), не member-access. | floating (codegen) | ✅ DONE |
| `[M-172.1-some-target-coerce]` | `Some(literal)` не инферил target generic-type (напр. `Option[MyStruct]` от `Some(MyStruct{...})`) — параллельный gap к `Ok(literal)` fix D85. Починен в Plan 172.1. | Plan 172.1 | ✅ DONE |
| `[M-173-priv-field-samename-bypass]` | ✅ **FIXED.** Одноимённое приватное поле типа (`type T { priv x int, pub x bool }`) обходило priv-гейт через method-call-путь — `self.x` внутри метода типа читал priv-поле как pub. Фикс: priv-гейт единообразен для field-access и method-dispatch. | floating (checker) | ✅ DONE |
| `[M-173.0-R2]` | Plan 173.0 R2: `scope.grow_children` tripwire — chunked stable-address storage для >16 детей скоупа (предотвращает realloc инвалидацию ссылок на child-скоупы). Реализован в Plan 173.0. | Plan 173.0 | ✅ DONE |
| `[M-174.1-parse-typeset-pathcall]` | Codegen gap: type-set-bounded generic pathcall (`fn f[T MyProtocol]`) — вызов статического метода через type-param не кодгенился (fall-through к легаси). Починен в Plan 174.1. | Plan 174.1 | ✅ DONE |
| `[M-175.1-variant-name-collision]` | Плоское пространство имён вариантов sum-типа: два типа в одном модуле с одинаковым именем варианта (`Reject`) давали CC-FAIL duplicate. Фикс: `RejectMismatch` для второго, документация конвенции. | floating (parser) | ✅ DONE |
| `[M-176-variant-ctor-method]` | Parser: PascalCase variant ctor в позиции method-call (`x.Variant(args)`) — greedy path collect уводил в `[P67-LEGACY]`. Починен: parser распознаёт variant-ctor по контексту method-call. | floating (parser) | ✅ DONE |
| `[M-187-d182-turbofish-new-nameonly-collision]` | Turbofish + default-arg + новый nameonly-параметр: `f[T](x=1)` и `f[T](x=1, y=2)` не различались при nameonly-диспетче. Починен в Plan 187. | Plan 187 | ✅ DONE |
| `[M-187-errorkind-parsejsonerror-variant-collision]` | `ErrorKind` variant arity collision: `ParseJsonError(msg)` и `ParseJsonError(msg,line)` — flat variant namespace не различал arity. Фикс: variant-ctor включает arity в диспетч. | Plan 187 | ✅ DONE |
| `[M-187-interp-to_str-fallback-valuerecord-recv]` | Interpreter: `to_str` fallback на value-record receiver (`str @to_str() -> @to_str()` self-call) — бесконечная рекурсия. Фикс: interp отличает value-record самовызов от внешнего `@to_str`. | Plan 187 interp | ✅ DONE |
| `[M-187-leaks-introspection]` | Утечка ресурсов в `introspection report_json` — parser/checker visitation не освобождал временные структуры при ошибке валидации. Починен в Plan 187 (arena-фиксация). | Plan 187 | ✅ DONE |
| `[M-187-nested-spawn-scope-var-cc-fail]` | `spawn` лексически вложенный в другой `spawn` — CC-FAIL «scope-queue out of scope»: runtime panic при парковке внутреннего fiber. Починен в Plan 187 (scope-chain propagation). | Plan 187 | ✅ DONE |
| `[M-187-sequential-2nd-request-hang]` | Sequential 2nd request hang: fiber-park после первого request не возобновлялся для второго (флагманский баг). Починен в Plan 187 (park/wake reset). | Plan 187 | ✅ DONE |
| `[M-187-weather-live-tls-diamond-blocked]` | Weather live: TLS diamond dependency — взаимная блокировка при TLS-рукопожатии в concurrent-фиберах. Починен (scheduler-очередь, non-blocking TLS). | Plan 187 | ✅ DONE |
| `[M-196-method-turbofish-block-rewrite-ice]` | ICE в `callnorm.rs` `try_normalize_cal` при method-turbofish + block-rewrite (`x.method[T]() { ... }`). Починен: block-rewrite guard на turbofish-пути. | Plan 196 | ✅ DONE |
| `[M-196-mono-block-notrailing-ret-ignored]` | Mono block без trailing return: codegen безусловно дописывал `return NOVA_UNIT` после тела mono'd функции, затирая реальный return. Починен: emit return только если block действительно unit-терминатор. | Plan 196 | ✅ DONE |
| `[M-196-probes-b10m-phase1c]` | Known-red probe b10m phase1c — регрессионный тест Plan 196 phase 1c (закрыт вместе с Plan 196). | Plan 196 | ✅ DONE |
| `[M-196-probes-b11al-terminal]` | Known-red probe b11al terminal — регрессионный тест Plan 196 (закрыт вместе с Plan 196). | Plan 196 | ✅ DONE |
| `[M-196-probes-b12q-terminal]` | Known-red probe b12q terminal — регрессионный тест Plan 196 (закрыт вместе с Plan 196). | Plan 196 | ✅ DONE |
| `[M-196-probes-b12r-terminal]` | Known-red probe b12r terminal — регрессионный тест Plan 196 (закрыт вместе с Plan 196). | Plan 196 | ✅ DONE |
| `[M-196-probes-b12s-terminal]` | Known-red probe b12s terminal — регрессионный тест Plan 196 (закрыт вместе с Plan 196). | Plan 196 | ✅ DONE |
| `[M-202-...]` | Generic-match scope-gap: `x` в match-арме generic-метода не резолвился как локальная переменная (scope-visibility). Починен в Plan 202. | Plan 202 | ✅ DONE |
| `[M-208-time-display-fmt-migration-gap]` | Duration `Display`/`Debug` сигнатура мигрирована на `Fmt` (Plan 208 Ф.2/D422). Реализовано в Plan 208. | Plan 208 | ✅ DONE |
| `[M-73.2-err-payload-consume]` | `Err`-payload consume-волна: pattern-биндинг `Err(consume e)` — consume-аннотация на payload-паттерне sum-variant. Реализовано в Plan 73.2. | Plan 73.2 | ✅ DONE |
| `[M-91.11-from-char-direct]` | `char.to_stringbuilder()` — fluent chain (`sb.append(ch)`), совместимость с D180. Реализовано в Plan 91.11. | Plan 91.11 | ✅ DONE |
| `[M-atomicint-record-field-typedef-collision]` | ✅ **FIXED.** `AtomicInt` как поле record — CC-FAIL duplicate typedef (typedef коллизия между `Nova_AtomicInt` и его value-record вариантом). Фикс: codegen различает value и non-value typedef. | floating (codegen) | ✅ DONE |
| `[M-boehm-...]` | Boehm GC large buffer retention при fiber-reuse — `net.c` не освобождал `close`-on-free буферы. Починен: free-on-close для GC-буферов (Plan 187/110.9). | Plan 187 | ✅ DONE |
| `[M-canceltoken-prelude-decl]` | `CancelToken` перенесён из builtins HashSet в формальные prelude-декларации — тип становится видимым для чекера/документации. Реализовано. | prelude | ✅ DONE |
| `[M-d216-unsafe-map-single-file-gaps]` | `unsafe`-атрибут отложен до per-overload энфорса A11-карты (D216). Single-file gaps: unsafe-маркировка не пропагировалась на FFI-импорты в однофайловом режиме. | floating (unsafe tracking) | P3 |
| `[M-d73-d77-retraction-migration]` | Retraction-миграция D73/D77-форм: старый синтаксис убран, кодовая база приведена к актуальной spec-форме. | spec cleanup | ✅ DONE |
| `[M-flagship-...]` | Флагманский маркер: `report_json_test` — расшифровка лога флагманского теста. Аналитический маркер (не баг, not actionable). | floating (analytics) | P3 |
| `[M-flagship-monotonic-now-bare-binding-ice]` | `Monotonic.now()` bare binding ICE: вызов статического метода на bare type-name (без скобок) в single-file контексте. Починен. | Plan 196 | ✅ DONE |
| `[M-flagship-spawn-capture-value-struct-ptr-mismatch]` | CC-FAIL: spawn capture value-struct передавался как `*` вместо inline value — mismatch в C-типе замыкания. Починен (value-record capture-path). | Plan 187 | ✅ DONE |
| `[M-flagship-spawn-throw-segfault]` | Segfault при `spawn throw` с multifield payload — раскладка payload'а на стеке не учитывала multi-slot эффект-значения. Починен. | Plan 187 | ✅ DONE |
| `[M-fmt-buf-module-path]` | `fmt_buf` module path isolation: отдельный модуль от `runtime.string_builder` для переиспользования без циклической зависимости. Реализовано. | std | ✅ DONE |
| `[M-freefn-named-default-arg-shift]` | Freefn named default arg shift: при nameonly-вызове аргументы со сдвигом пропускали default filler. Починен (call-site arg alignment). | Plan 196 | ✅ DONE |
| `[M-fs-real-io-bare-test-block-sched-park]` | Реальный Fs I/O в bare test блоке — fiber-scheduler scope/slot issue: park-слот не инициализирован для синтетического скоупа. Починен (scheduler fallback). | Plan 187 | ✅ DONE |
| `[M-generic-method-self-recursive-return]` | Self-recursive generic-enum return: mono generic-метод, возвращающий generic-тип того же enum — бесконечная рекурсия mono. Починен (recursion guard). | Plan 196 | ✅ DONE |
| `[M-hmac-array-repeat-literal-parser]` | Hmac: `[0; 32]` (array-repeat literal) не поддерживался парсером — использован явный литерал `[0,0,...,0]`. Аналитический маркер (parser gap). | floating (parser) | P3 |
| `[M-json-byte-peek]` | Json lexer: `peek()`/`advance()` возвращали `Option[char]` вместо `Option[u8]` — потеря байтов на UTF-8 токенах. Фикс: `Option[u8]`. | std/json | ✅ DONE |
| `[M-json-escape-bf-empty]` | Json: `\b`/`\f` escape последовательности декодировались в пустую строку. Фикс: `\b`→`U+0008`, `\f`→`U+000C`. | std/json | ✅ DONE |
| `[M-json-lexer-byte-cursor]` | Json lexer: `pos` был codepoint-курсор, не байтовый — ошибки позиционирования на non-ASCII. Фикс: байтовый курсор. | std/json | ✅ DONE |
| `[M-md5-array-repeat-literal-parser]` | MD5: `[0; 16]` / `[0; 16]u32` (array-repeat literal) не поддерживался — использован явный литерал. Аналитический маркер (parser gap, общий с M-hmac/M-sha1). | floating (parser) | P3 |
| `[M-no-silent-nova-int-fallback]` | Silent `nova_int` fallback при нерезолвящемся типе заменён честным `E7001` compile-error. Починен. | Plan 196 | ✅ DONE |
| `[M-parfor-capture-callee-name-collides-std-local]` | `parfor` capture: stale `var_types` запись с чужим param-именем — коллизия в parallel-for capture. Починен (scope-cleanup при parfor). | Plan 187 | ✅ DONE |
| `[M-semver-trailing-dash-plus]` | Semver: trailing `-`/`+` различается от отсутствия pre-release/build metadata. Фикс: trailing dash — пустой pre-release; trailing plus — пустой build. | std/semver | ✅ DONE |
| `[M-serde-encode-pointer-op-regression]` | Serde encode: `E_POINTER_OP_USE_METHOD` на blanket `to_str` — регрессия при переходе на blanket-реализацию. Починен (access path для blanket methods). | Plan 196 | ✅ DONE |
| `[M-set-from-iter-self-new-default-arg-backfill]` | `Set.from_iter`: `Self.new()` в generic-static теле — callnorm gap (default-arg не бэкафилился в generic context). Починен. | Plan 196 | ✅ DONE |
| `[M-sha1-array-repeat-literal-parser]` | SHA1: `[0; 20]` / `[0; 80]u32` (array-repeat literal) не поддерживался — использован явный литерал. Аналитический маркер (parser gap, общий с M-hmac/M-md5). | floating (parser) | P3 |
| `[M-vec-spelling-consume-block-body-untyped]` | Vec-spelling: consume block body без явного типа — inference gap при инициализации Vec из consume-block. Починен. | Plan 196 | ✅ DONE |
| `[M-vr-binop-wrapper-decl-order-standalone-cu]` | Value-record arithmetic: DCE-seed терял методы при standalone build — wrapper fn для binary operator не декларировался в нужном порядке. Починен (decl-order fix). | floating (codegen) | ✅ DONE |
| `[M-str-from-utf16-static-conversion]` | **OPEN 2026-07-30 (Ф.0-инвентарь конверсионного окна):** `str.from_utf16(units []u16)` (`std/src/encoding/utf16.nv:76`) — вероятное нарушение §1а: `[]u16` — значение-ресивер, тот же класс, что уже ретрактированный `str.from_bytes`. Кандидат: `[]u16 @to_str_utf16()` или лексикализованное имя по решению владельца. Не мигрировано волной 2026-07-30 (вне мандата). Приоритет P3. |
| `[M-polaris-from-request-static-conversion]` | **OPEN 2026-07-30 (та же Ф.0):** 11 сайтов `*.from_request(req ServerRequest)` в nova-polaris (AddNoteReq/WebSocketUpgrade/Multipart/Bearer/BasicAuth/CookieJar/Bytes/Text/Headers/Req/GetItemBundle) + пограничный `StreamBody.from_chunks` — форма структурно родня запрещённому статик-parse. **РЕШЕНИЕ ВЛАДЕЛЬЦА 2026-07-31: «Согласен» — extractor-концепт уровня Axum FromRequest, ЛЕГАЛЕН, §1а не трогаем, миграции НЕ будет.** (17 сайтов SocketAddr.from_str мигрированы интегратором 2026-07-31 отдельно — это другая ось.) Плюс 17 сайтов `SocketAddr.from_str` в polaris потребуют миграции на `@to_socket_addr` при бампе пина std (декла удалена волной 2026-07-30). Зона polaris. |
| `[M-net-lookup-port-u16-residual]` | **OPEN 2026-07-30:** остаточная связанная группа `port u16` после перевода публичного API на `int + requires` (решение владельца): `net/dns.nv:32 resolve()`, `net/effect.nv:67 Net.lookup`, `net/tcp.nv:452` + `net/mock.nv:71` (реальный и mock-хендлеры одного эффект-опа). Эффект-сигнатура + 2 реализации + обёртка = ОДИН атомарный фикс (менять только вместе). Приоритет P3. |
| `[M-mono-fn-decls-module-qualified-key]` | **OPEN 2026-07-30 (№129, отложенный полный фикс):** ключ `mono_fn_decls` = голое имя fn; две одноимённые module-private generic-функции из разных модулей ЯЗЫКОВО легальны, но сталкиваются. Сейчас — честная ошибка [E_MONO_FN_KEY_COLLISION] (конверсия из miscompile «чужое тело, 222 вместо 111»); ПОЛНЫЙ фикс = module/file-qualified ключ через ~10 read-сайтов. Отдельное компиляторное окно. P2. |
| `[M-p67-legacy-multiblanket-return-type]` | **OPEN 2026-07-30 (№129, найдено репро):** вызов метода при НЕСКОЛЬКИХ blanket-кандидатах падал в `internal error [P67-LEGACY] method call return type unknown` (emit_c:58819) — пробел резолва return-type в чекере для multi-candidate blanket'ов. После №129 сталкивающиеся кандидаты дают честную коллизию раньше, но не-коллидирующие multi-candidate формы могут упираться в тот же пробел. Чекер-канал. P2, разбор при №149 шагах 2-4. |
| `[M-attr-order-stable-unverified-export]` | **OPEN 2026-07-31 (окно 236):** порядок токенов атрибутов жёсткий и недокументированный — работает только `export #unverified\nfn ...`; формы `#unverified\nexport fn` и `export\n#unverified\nfn` — parse error. Либо принять все порядки, либо задокументировать канон в спеке + внятная диагностика. P3. |
| `[M-lint-redundant-to-str-in-interpolation]` | ✅ **РЕШЕНО 2026-07-31 (окно p-lints): правило W_REDUNDANT_TO_STR_INTERP заведено (+20 находок — линт-свип подберёт); сосед про операторные формы ОТВЕРГНУТ с доказательством (активно-неверный совет на neg-фикстурах).** Исходно: **OPEN 2026-07-31 (ревью владельца по bigdecimal):** `${x.to_str()}` в интерполяции — лишний вызов, интерполяция сама диспетчеризует to_str/Display пользовательского типа (проверено пробой на BigInt). Линт W_-класса (семья manual_*): матчить `.to_str()`-хвост выражения непосредственно внутри `${...}`. Заодно: `.plus(...)/.times(...)/.minus(...)` там, где есть операторный сахар (@plus/@times — зарезервированные имена, 03-syntax.md:1188) — кандидат на соседний W_METHOD_FORM_OF_OPERATOR (проверить шум на std перед включением). Сайты bigdecimal уже переписаны интегратором (bigint внутренности — при том же линт-окне). P3, компилятор-очередь. |
| `[M-reflect-derive-generic-wrapper-field-null-shape]` | ✅ **ЗАКРЫТО 2026-07-31 (приёмка интегратора: сверка с №146 подтверждена, фикстуры влиты; пример уже на Some(...) hand-built от окна 2223) (worktree `nova-preflect`, ветка `p-reflect-nullshape`, sonnet) — ПОХОЖЕ НА ДУБЛИКАТ №146, УЖЕ ЗАКРЫТОГО, рекомендация закрыть, финальное слово за интегратором.** Полаrisовское наблюдение (`extract_test.nv`'s `GetItemBundle`-комментарий) датировано 2026-07-27 — ДО фикса №146 (`[M-reflect-fieldwalk-generic-field-not-monomorphized]`, commit `263440459d8`, 2026-07-29, ЗАКРЫТ `77ea6df12`, 2026-07-30), который чинит РОВНО этот механизм (field-walk терял type-args генерик-поля → незаквалифицированный `Path([name,"reflect"])` → немономорфизированный `return NULL;`-заглушка). Репро-усилие этой волны: собрал компилятор из ТЕКУЩЕГО HEAD (уже содержит №146), воспроизвёл ТОЧНУЮ форму (`value`-generic-обёртка с HAND-WRITTEN `#impl(Reflect)`-помеченным `.reflect()` — как реальные `PathParam[T]`/`Query[T]`/`Json[T]` в extract.nv, НЕ auto-derived обёртка) как поле бандла — сначала одна обёртка, затем ТРИ разных обёртки в одном бандле (canon `GetItemBundle`-форма id/filter/payload/raw, 4 поля), затем два бандла с одной обёрткой разных T в одном CU (cross-mono). ВСЕ варианты: глубокие content-ассерты на вложенный `TypeShape` (не только non-NULL) — PASS; сгенерированный C (`--keep-artifacts`) подтверждает корректный TurboFish-резолв на РЕАЛЬНО мономорфизированный символ (`Nova_<Wrap>____NovaValue_<Inner>_p_static_reflect`) с НАСТОЯЩИМ телом — НЕ на общую `Nova_<Wrap>_static_reflect` заглушку (та существует В КАЧЕСТВЕ отдельного, невостребованного placeholder'а, тело `return NULL;`, но field-walk её не зовёт). Буквальный Ф.0-репро из брифа (`type W[T] value {...}` БЕЗ hand-written `.reflect()`, чистый `#impl(Reflect)` auto-derive НА generic-получателе) — тоже проверен: даёт ЧЕСТНУЮ compile-time ошибку `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`/`E_IMPL_MISSING_METHODS` (др. класс, уже документирован как отдельный пробел в `reflect_generic_field_walk_pos.nv`'s doc-comment), не молчаливый NULL. Регресс-фикстуры (не воспроизводят баг, ЗАКРЕПЛЯЮТ корректное поведение): `spec_tests/conformance/m_rfl_generic_value_wrapper_field_pos.nv`, `spec_tests/conformance/m_rfl_three_wrapper_bundle_pos.nv`. Исходно: **OPEN 2026-07-31 (комментарий примера polaris 03-json-api: «Reported to integrator» — репорт письменно НЕ доходил, зарегистрирован при разборе):** auto-derive `Reflect` на бандле, чьи поля — generic-wrapper-инстансы (`PathParam[T]`/`Query[T]`/`Json[T]`), эмитит вызов НЕмономорфизированного символа поля — заглушка возвращает NULL вместо реальной инстанс-shape; потребление NULL-shape (openapi-эмиттер, GC) роняло живой сервер (fiber stack overflow). НЕ №139 (закрыт) — дыра уровнем выше в той же auto-derive-машине. Пока обход: `req_shape: None` в примере. Блокирует полные OpenAPI-схемы бандлов → зона 222.8/222.3, ДО ТЕГА (A-V11). P1. |
| `[M-missing-static-method-p67-ice]` | **OPEN 2026-07-31 (вскрыт миграцией polaris):** вызов НЕСУЩЕСТВУЮЩЕГО статик-метода (`SocketAddr.from_str` после удаления деклы) = ICE `emit_c.rs [P67-LEGACY] Path call return type unknown` вместо честной E_UNKNOWN_METHOD с подсказкой. ICE на пользовательском вводе недопустим (§4а). Родня P67-семьи (№157-ICE закрыт, [M-p67-legacy-multiblanket-return-type] открыт) — кандидат добить всю семью одним окном. P2. |
| `[M-fn-return-value-bound-var-call-broken]` | **OPEN 2026-07-31 (Ф.0-разведка окна 222.3):** вызов через локальную переменную, связанную с результатом НЕ-generic функции, возвращающей `fn(...)->...`, разбит — codegen эмитит вызов несуществующей `nova_fn_<var>` вместо диспетча через сохранённое замыкание (ld: undefined symbol). Репро p140_closure_var_call*.nv в отчёте окна. Родня №140-семьи (данные subst/замыканий не текут в локалы). P2. |
| `[M-stored-field-closure-call-arity-env-lost]` | **OPEN 2026-07-31 (попутно, окно №140):** вызов fn-параметра, сохранённого полем структуры (`Router.h(v1)`-класс), теряет env-аргумент замыкания — «too few arguments, expected 2, have 1». Родня №140-семьи. P2. |
| `[M-returned-closure-direct-call-p67]` | **OPEN 2026-07-31 (попутно, окно №140):** прямой вызов возвращённого замыкания без промежуточной переменной `gen_call(...)(args)` — ICE [P67-LEGACY] «Path call return type unknown (no parts)». P67-семья (кандидат в общее окно семьи). P2. |
| `[M-serde-decode-errors-option-vec-ctype-mismatch]` | **OPEN 2026-07-31 (вскрыт окном №176, δ0-доказан на до-фиксном бинаре):** `std/src/encoding/serde/decode_errors_test` CC-FAIL — `Option[Vec[str]]` vs `Option[Vec[int]]` C-type mismatch в тест-лейне encoding. Пре-существующий, generic-mono семья. P2. |
| `[M-lint-interp-subexpr-span-offset]` | **OPEN 2026-07-31 (попутно, окно p-lints):** sub-parsed выражения внутри `${...}` несут span с байт-офсетом, локальным к substring'у интерполяции (прежний фикс [M-str-interp-wrong-file-id] починил только file_id, не офсет) — любой диагностический канал, указывающий внутрь интерполяции, репортит line:col у начала файла; для машинного Suggestion (будущий nova fix) офсеты опасно неверны. parser/mod.rs ~11868. P2. |
| `[M-int128-blanket-dispatch-first-by-name]` | **✅ ЗАКРЫТ 2026-08-01 (окно p-int128, sonnet):** `.find()` → отсортированный перебор кандидатов с фильтром по bound-семье ресивера (emit_c ~43342); фикстура m128_blanket_family_dispatch_pos (две disjoint-семьи в одном CU). Было:** blanket-диспетч `blanket_key_opt` (emit_c ~42978) выбирает кандидата ПЕРВЫМ-ПО-ИМЕНИ — диагноз в шапке `i64 @to_i128()` (std/src/math/int128.nv, ~85-100). Родня №129-семьи, но отдельный сайт (сознательно исключён из окон №129/genmono). Вместе с ним — `[M-176-math-int128-no-intrinsics]` (runtime assertion вместо compile-time в int128): ОБА в компиляторное окно сразу после 234-A/Reflect. P1-связка 234. |
| `[M-fs-loop-affinity-unproven]` | **OPEN 2026-07-31 (аудит extern-поверхности):** fs.c структурно свободен от класса `[M-183-net2-loop-affinity]` (fd — голый int, одноразовый uv_fs_t-req на текущем loop), но в отличие от net.c это НЕ подтверждено ни doc-комментарием авторов, ни тестом. Нужен явный тест: open на воркере A → work-steal → чтение на воркере B; либо LOOP-AFFINITY-комментарий по образцу net.c:37-51. P3. |
| `[M-os-env-concurrent-access-race]` | **OPEN 2026-07-31 (аудит extern-поверхности):** `os_env_get/set/...` зовут getenv/setenv над глобальным environ БЕЗ блокировки — классический glibc-хэзард при КОНКУРЕНТНЫХ вызовах из разных воркеров (мутатор инвалидирует указатель параллельного getenv). НЕ thread-affinity (#thread_affine не решает) — класс shared-global-state: нужен process-wide mutex вокруг env-мутаторов в os_env.h либо документированное «env — однопоточно». P2. |
| `[M-neg-cast-receiver-blanket-dispatch]` | **✅ ЗАКРЫТ 2026-08-01 (окно p-int128, чекер-канал):** BoundCtx::infer_arg_ty (types/mod.rs ~25004) не имел Unary-ветки — добавлена Unary{Neg|Not|BitNot} (type-preserving, рекурсия в операнд) → resolved_callees заполняется, конкретная перегрузка побеждает blanket. Пин: (-5 as int).to_i128() в int128_test. Было:** receiver-форма `Unary(Neg(Cast))` — буквально `(-N as i64).mk()` — уводит диспетч в blanket вместо конкретного метода; голый `(N as i64).mk()` корректен. Это НЕ №129-класс (проверено фактом). Именно эта форма блокирует blanket'ы SignedInts/UnsignedInts (234 часть B) и была в исходном №149-отчёте. Диагноз/гипотеза корня — CHECKPOINT_234.md ветки p234-bitwise-2. В int128-связку P1. |
| `[M-shl-shr-user-type-no-dispatch]` | **✅ ЗАКРЫТ 2026-08-01 (окно p-int128):** resolve_shift_dispatch в codegen/bitwise_ops.rs (@minus-паттерн: гетерогенный второй параметр n int; плоский + generic-mono с ____-гардом); фикстуры m128_shl_shr_user_type_pos + neg. Было:** `<<`/`>>` на пользовательском типе не диспетчатся вовсе — та же дыра, что была у &/|/^ до Ф.1; фикс тем же fast-path-зеркалом. В int128-связку. |
| `[M-neg-not-selectors-dce-gap]` | **✅ ЗАКРЫТ 2026-08-01 (окно p-int128):** collect_used_names: neg/not по своему UnOp + shl/shr безусловно (без них тела @shl/@shr вовсе не эмитились). Фикстура m128_neg_not_operator_only_pos. Было:** @neg/@not — тот же reachability-DCE-гэп collect_used_names, что был у @bit* (тела убивает DCE при только-операторном использовании). Малый фикс класса Ф.1(б). |
| `[M-negative-i64-to-uint-cast-clamps-zero]` | **✅ ЗАКРЫТ-НЕ-БАГ 2026-08-01 (окно p-int128, сверка со спекой):** D130 (02-types:5240-5263) НОРМАТИВНО требует сатурации в 0 только для пары int→uint (AMEND-исключение из D54 bit-truncate); n as u64 при n<0 корректно реинтерпретирует (проверено пробами через ==, не to_str — тот сам врёт, см. [M-u64-uint-to-str-prints-signed]). Заявление маркера не подтвердилось. Было:** `as uint`/`as u64` для значений с отрицательным i64 bit-pattern клампит в 0 через nova_int_to_uint вместо raw bit-reinterpret — пред-существующий; семантику каста сверить со спекой (D-решение по кастам) и починить/задокументировать. |
| `[M-detach-box-capture-cancel-token-reachable]` | **НЕ ВОСПРОИЗВОДИТСЯ 2026-07-31 (окно p-box-capture, sonnet, 354k):** на main 2e45ab920 не воспроизводится ни изолированно (7 фикстур), ни verbatim-копией polaris с патчем из комментария serve() (доп. гейты зелёные). Вероятная причина ухода — 744e9b6d5 (supervised TLS, 2026-07-30); репорт serve-окна, видимо, со stale-бинарём. Гвард-фикстура m_boxcap_detach_cancel_token_pos влита. ЗАКРЫТИЕ — по успешному возврату graceful shutdown (окно p-shutdown, идёт). Было: P1 OPEN (окно serve; блокировал 222.22): как только CancelToken-значение достижимо в функции, чьё тело содержит многозахватный `detach consume ... { ... inflight.try_acquire() ... }` — codegen теряет boxing-символ НЕСВЯЗАННОГО захвата: `use of undeclared identifier '_nova_detach_0_box_inflight'`. Репро в комментарии serve() (nova-polaris/src/net/serve.nv, ветка p-serve — влито в master). Реализация shutdown ГОТОВА и откачена — после фикса вернуть по коммит-истории окна. |
| `[M-effect-handler-mutex-hashmap-value-capture]` | **✅ ЗАКРЫТ 2026-08-01 (окно p-soundness-pack п.6):** воспроизвёлся в Router/mut-параметр-форме; корень — emit_lambda: классификация mut-захвата не видела ref_params (mut-value параметры, pointer-ABI) и ref_params не был scoped per-lambda; починено, фикстура m_mutparam_closure_capture_mismatch_pos, CC-FAIL→PASS test-build. Ранее: НЕ ВОСПРОИЗВОДИЛСЯ 2026-07-31 (box-окно): оба варианта (ro-фабрика и mut-локаль в escaping-фабрике) компилируются и работают, C проверен глазами. Гвард-фикстура m_boxcap_effect_handler_mutex_hashmap_pos влита. ЗАКРЫТИЕ — по эффектной форме Metrics (окно p-shutdown). Было: P1 OPEN (родственная форма): стейтфул effect-хендлер, замыкающий value-record с Mutex+HashMap-полями — `assigning to NovaValue_T from incompatible type NovaValue_T *`; воспроизводится голым реестром без detach. Репро в файл-баннере nova-polaris/src/metrics.nv. Блокирует эффектную форму Metrics (222.23); рабочая безэффектная форма влита. Один класс с маркером выше (box/ptr-форма захвата) — ОДНО окно на оба. |
| `[M-arith-binop-generic-receiver-no-mono-register]` | **✅ ЗАКРЫТ 2026-08-01 (окно p-operator-unify, sonnet, план opunify — BRIEF_opunify.md):** единая таблица `codegen/operator_dispatch.rs::BINOP_TABLE` + один резолвер `resolve_binop_dispatch` для ВСЕХ десяти бинарных операторных перегрузок (было: `bitwise_ops.rs`, git mv, история сохранена) — генерик-mono ветка (`register_mono_method_instance`) теперь общая для `+ * / % & | ^ << >>`, `+ * / %` получили её бесплатно вместе с остальными. Фикстура `m_opu_arith_generic_mono.nv` (generic `MOpuArithBox[T]`, все пять арифметических операторов на mono-ресивере) — compile+link+run+assert зелёные. Было: **OPEN 2026-07-31 (приёмка 234, родня закрытой Bit*-регрессии):** плоские D46-ветки `+`/`*`/`/`/`%` (emit_c.rs ~34199-34235) на mono-имени генерика (`____`) эмитят вызов `T____X_method_plus/…` БЕЗ `register_mono_method_instance` — undefined symbol, если генерик-тип объявит `@plus`-семью (сегодня в std таких нет — у `Set` только `@minus`/Bit*, оба с генерик-ветками). Bit*-семья загарждена гардом при приёмке 234 (фикстура m234_set_bitor_generic_mono); арифметике нужен тот же гард+генерик-ветка. Компиляторная очередь. |
| `[M-vec-seq-missing-import-ice-not-e7320]` | **OPEN 2026-07-31 (ревью флагмана):** `.map(...)` на `Vec[UserType]` БЕЗ явного `import std.collections.vec_seq.{map}` падает ICE `[P67-LEGACY] method call .map return type unknown` (emit_c.rs:58906) вместо чистой `E7320 no method`; на `Vec[str]` — корректная E7320. Репро: два теста, user-тип vs str, без импорта. Диагностика обязана быть E7320 + подсказка «eager combinators need import std.collections.vec_seq» (M-153-класс, комбинаторы вне prelude — осознанно). Компиляторная очередь, диагностический канал. |
| `[M-try-map-err-chain-loses-payload-type]` | **✅ ЗАКРЫТ 2026-07-31 (той же волной, срочно по владельцу):** infer_unwrapped_call_type (types/mod.rs, ConsumeCtx): `map_err`-звено прозрачно для Ok-payload — рекурсия в receiver (Ident → var_unwrapped_types, Call → рекурсивно); Err-companion честно None (map_err меняет E). Фикстура m_tmec_map_err_try_consume; aggregate.nv переведён на форму `map_err+?`. Было: **P1 OPEN 2026-07-31 (ревью флагмана, форма владельца):** `consume x = expr.map_err(\|e\| ...)?` — чекер НЕ подставляет генерики возврата `@map_err[F]` (Result[T,F] приходит стёртым), Try-развёртка отдаёт None/`Result` вместо payload-типа → D432-аффинность @cleanup-типа не видна → ложный `D133-not-consumed` (в диагностике «тип \`\`»/«тип `Result`»). Голый вызов без map_err работает (репро-пара). Блокирует канонную цепочную форму `map_err+?` на consume-типах (fetch_one aggregate.nv — обход match'ем с маркером по месту). Чекер-канал, семья подстановок №170/№140. |
| `[M-lsp-client-write-after-destroy-on-server-restart]` | **OPEN 2026-07-31 (скрин владельца):** расширение nova-lang-local 0.2.0 при смерти/подмене nova-lsp.exe (пересборка бинаря) продолжает слать file-events в разрушенный поток — «Cannot call write after a stream was destroyed» (vscode-jsonrpc ril.js). Сервер сам переподнимается здоровым (warm-start 3400/3405, 1.4с). Нужно: отцеплять file-event watcher до рестарта / глушить запись после destroy. editors/-зона. |
| `[M-tcp-read-to-vec-rename-read-bytes]` | **РЕШЕНИЕ ВЛАДЕЛЬЦА 2026-07-31, исполнить волной после приёмки полярис-хвоста:** `TcpStream @read_to_vec` → `@read_bytes` (симметрия пары content-имён с `@read_text`; прецедент имени — `ReadBuffer.@read_bytes`). Жёсткое переименование без алиаса (до релиза): decl в std/net/tcp.nv (+ half'ы, если у них та же форма), ~14 сайтов nova-репы (std/examples/2 standalone-фикстуры), 35 .nv-сайтов пакетных реп (polaris/tls/...), амендмент D-блока со списком API (04-effects ~6475) + пример 03-syntax ~8831. НЕ исполнять, пока окно A-V11-хвоста не влито (оно правит socket_echo_smoke и билдится об std главной репы). |
| `[M-json-object-map-literal-protocol]` | **OPEN 2026-07-31 (ревью владельца, 05-auth):** экспортировать у `JsonObject` (std/src/encoding/json.nv, priv упорядоченный словарь) конструкторы map-литерал-протокола (`with_capacity` + `mut @insert_new`) — тогда `ro fields JsonObject = { sub, exp: 5e9 }` десугарится напрямую, а `fields` в аргументе `claims JsonValue` sum-коэрцируется (единственный вариант с payload JsonObject) — целевая форма владельца `Jwt.encode_hs256(secret, fields)` без обёртки `JsonValue.object(...)`. Сейчас достижимая половина применена (05-auth: литерал в HashMap + object(...)). std-API-очередь, после blanket @to_json. |
| `[M-shl-shr-non-self-return-local-infer-segfault]` | **OPEN 2026-08-01 (попутно, окно p-int128; ЛАТЕНТНЫЙ — в std носителей нет):** `fn[T] Type[T] @shl(n int) -> int` (НЕ-Self возврат, generic-mono ресивер) — тип локали `ro x = recv << n` инферится как указатель на ресивер вместо int → wild-pointer → SEGFAULT (сырой краш). Плоский ресивер с тем же возвратом не падает. Минимальное репро — в комментарии m128_shl_shr_user_type_pos.nv. Компиляторная очередь (канал типов локалей). |
| `[M-u64-uint-to-str-prints-signed]` | **OPEN 2026-08-01 (окно 234 заметило, окно p-int128 подтвердило независимо):** `to_str()`/`println` для `u64`/`uint` со старшим битом печатают знаковое (`-6` вместо `18446744073709551610`). Ломает диагностику беззнаковых; чинить в конверсии числа→строка (runtime/string либо emit-выбор форматтера по типу). Компиляторная очередь. |
| `[M-router-handler-mut-capture-escape-soundness]` | **P1 ЧАСТИЧНО 2026-08-01 (окно p-soundness-pack): §2 field-launder ЗАКРЫТ (E_READONLY_COERCE Member-канал + #share-исключение; neg-фикстуры m_ro_launder_l/m; вскрыл и починил launder-сайты metrics/openapi/mock). §3 escaping-closure ОСТАЁТСЯ ОТКРЫТ — нужен общий escape-анализ (риск ложняков на map/filter HOF), дизайн-окно после ICE-пачки.** Изначально: P1 OPEN 2026-08-01 (вопрос владельца по 09-graceful):** `mut log []str`, захваченный в fn-литералы хендлеров Router и в BackgroundTasks-замыкание, компилируется под --strict-effects, хотя хендлеры исполняются на файберах разных соединений → гонка данных (push vs len/чтение). Класс: escaping closure через РЕГИСТРАЦИЮ (не spawn/detach-синтаксис) — 5 слоёв M:N-энфорса его не видят. Нужен энфорс на mut-захват в fn-литерале, уходящем в не-локальный приёмник (escape_analyze-канал). **ВТОРОЙ канал той же дыры (владелец 2026-08-01, metrics.nv): ro-launder ПОЛЕЙ — `mut lock = @lock` / `mut gauges = @gauges` из ro-метода легально вымогают mut на разделяемое состояние (№106 закрыл только pattern-bind канал, field-read канал открыт); энфорсить вместе, ту же неготивную фикстуру дополнить field-launder формой.** Пример 09-graceful чинится на Mutex окном p-shutdown. **Приёмка ОБЯЗАНА включать НЕГАТИВНУЮ фикстуру (требование владельца 2026-08-01): mut-локаль, захваченная в fn-литерал, который регистрируется/уходит из скоупа (форма Router-хендлера) → compile-error с E-кодом (EXPECT-маркер по test-conventions), плюс pos-двойник с Mutex/#share-типом — компилируется.** M:N-звучность, компиляторная очередь ВЫСОКО. |
| `[M-lint-manual-max-min-if]` | **OPEN 2026-08-01 (ревью владельца, bigint.nv:61):** нет линта на ручной max/min — `if x > y { x } else { y }` (и аргументы-выражения, напр. `a.len()`) должен предлагать `x.max(y)` (W_MANUAL_MAX_MIN; зеркально `<` → min; формы if-expr и match). std @max/@min есть на всех числовых (runtime/defaults.nv). lints.rs, AST-правило. |
| `[M-option-int-cast-u64-cc-fail]` | **OPEN 2026-08-01 (отчёт окна 237/BigFloat):** `(x.to_int() as u64)` при `x: Option[int]` проходит чекер, но валит C-компиляцию — обход оставлен в bigfloat.nv (грепнуть по маркеру при фиксе). Уточнить минимальное репро при взятии в работу. Компиляторная очередь. |
| `[M-bigint-family-migrate-named-tuple]` | **РЕШЕНИЕ ВЛАДЕЛЬЦА 2026-08-01, исполнить после ICE-фикса:** `type BigInt value {sign, limbs}` → named tuple `type BigInt(sign Sign, limbs []u32)` (D215: семантика та же — stack value + copy; выигрыш: позиционный конструктор, явный контракт размещения). Заодно оценить BigDecimal/BigFloat (те же value-records). Блокеры: [M-named-tuple-field-accessor-on-call-ice] (f().field — ICE; BigInt-код на цепочках) и [M-named-tuple-positional-defaults]. Миграция механическая: декларация + сайты `{ sign: …, limbs: … }` → `BigInt(…, …)`; nova-bigint репа. |
| `[M-lsp-inlay-hint-arg-anchor-inner-expr]` | **OPEN 2026-08-01 (скрин владельца, bigdecimal.nv:109):** inlay-hint имени параметра якорится на ВНУТРЕННЕЕ первичное выражение аргумента, а не на корень: `factor.div((2).to_bigint())` рисуется как `div((other: 2).to_bigint())` вместо `div(other: (2).to_bigint())` — читается как чужой код. nova-lsp, зона 104.x (inlay hints): позиция = span корня arg-выражения. |
| `[M-operator-dispatch-unify-eq-compare-family]` | **OPEN 2026-08-01 (следующая волна унификации, честный остаток окна p-operator-unify):** `== != < <= > >=` (@equal/@compare, D237) не в BINOP_TABLE — протокол-диспетч с десятками веток по носителям (str/Vec/Option/tuple/…). Отдельное проектирование формы записи в таблицу; byte-identical регресс обязателен. |
| `[M-operator-dispatch-unify-value-record-path]` | **OPEN 2026-08-01 (тот же остаток):** value-record путь (`NovaValue_X`, nova_vr_binop_*) не переведён на общий резолвер operator_dispatch — отдельная ветка; перевод с регрессом на Duration/Timestamp/Monotonic. |
| `[M-generic-reflect-call-inside-sibling-struct-literal]` | **OPEN 2026-08-01 (окно p-polaris-tail, sonnet):** `T.reflect()` (generic typevar), вписанный ПРЯМО в поле литерала СОСЕДНЕГО типа (`RouteInfo{ req_shape: Some(T.reflect()) }`), мономанглится в символ по имени типа литерала (`Nova_RouteInfo_static_reflect`) — undefined link. Обход: вынести в `ro shape = T.reflect()` до литерала (задокументирован в polaris src/extract.nv у @post_typed_h). Класс мономангла static-вызова в контексте record-литерала. Компиляторная очередь. |
| `[M-channel-try-recv-ro-binding-p67-ice]` | **OPEN 2026-08-01 (окно p-polaris-tail):** `ro x = reader.try_recv()` внутри обычной fn (не test-блока) → internal error [P67-LEGACY] method call .try_recv return type unknown; прямой вызов в выражении работает. Родня [M-channel-generic-elem-type]. Стабильное репро — polaris src/ws/rt/socket_echo_load_smoke.nv (обход по месту). Чекер-канал (тип Channel-generic в ro-биндинге). |
| `[M-unused-import-lint-ignores-impl-attr]` | **OPEN 2026-08-01 (хвост + флагман, два независимых наблюдения):** unused-import линт не засчитывает использование имени через `#impl(X)`-атрибут — ложные warning'и на Serialize/Reflect/FromRequest/… (флагман main.nv:30, пример 03 polaris — 8 имён). Линт-канал: собирать имена из атрибутов #impl при подсчёте использований. |
| `[M-log-levels-standard-plus-threshold]` | **✅ ЗАКРЫТ 2026-08-01 (волна p-observability): Log 5 уровней+Level enum+POLARIS_LOG-порог+префиксы; warn-переклассификация admission/anti-flood; capture_log под Mutex. БылоРЕШЕНИЕ ВЛАДЕЛЬЦА 2026-08-01 (модуль обязан покрывать общепринятый стандарт):** Log-эффект polaris → 5 уровней trace/debug/info/warn/error (оп на уровень; эталон — Rust log-crate), порог в хендлере (`Level` enum, `real_log(min Level)`, печать при level>=min, префикс уровня в строке), дефолт #default_handler = Info, env-переопределение POLARIS_LOG; `capture_log` ловит всё + Mutex[[]str] (фикс mut-захвата, см. P1 router-mut-capture); сайты serve: admission-503/anti-flood/Retry-After → warn. Амендмент 222.20 §Q3 той же волной. Исполнение: polaris-волна «канон наблюдаемости» сразу после окна звучности (вместе с metrics-каноном). |
| `[M-lint-all-fields-priv-collapse]` | **OPEN 2026-08-01 (ревью владельца, polaris Part):** нет линта «все поля объявлены priv по отдельности → предлагать тип-уровневый `type X [value] priv {}` (D281)». W_ALL_FIELDS_PRIV, fix-it: перенос priv в заголовок + снятие с полей. Сайты polaris (13 типов) схлопнуты руками при ревью; std/http/bigint прочесать при внедрении правила. lints.rs, AST-правило. |
| `[M-use-contextual-keyword-router-use]` | **РЕШЕНИЕ ВЛАДЕЛЬЦА 2026-08-01:** `use` сейчас глобально зарезервировано лексером (KwUse) ради D39-делегирования внутри типов (`use field Type` / `use Protocol`) — понизить до КОНТЕКСТНОГО слова (как bench): ключевое только в позициях делегирования, свободное как имя метода/идентификатор. Затем polaris: `Router @layer` → `@use` (Express/Axum-идиома), сайты+доки+примеры. D-амендмент (D39/раздел keywords) тем же слиянием. Компиляторная очередь после пакета звучности. |
| `[M-editor-grammar-keyword-sync]` | **OPEN 2026-08-01 (скрин владельца: value без подсветки):** value/enum/bench отсутствовали в tmLanguage keyword-паттерне — добавлены (editors/vscode). Остаток: сверить ПОЛНЫЙ список слов лексера (lexer/mod.rs keywords) с грамматикой одним проходом + tree-sitter грамматика (104.7) той же сверкой; завести страж-скрипт сверки списка (guards) чтобы не расходились. |
| `[M-detach-box-while-loop-read-after]` | **P1 OPEN 2026-08-01 (окно p-shutdown, sonnet; НОВАЯ воспроизводимая форма box-класса):** mut-локал, захваченный в `detach{}` внутри `while`, читаемый ПОСЛЕ цикла в той же функции → `use of undeclared identifier '_nova_detach_0_box_<name>'`. Обход: цикл в отдельную функцию с параметром. Репро в комментарии serve() (polaris master). Передано окну p-soundness-pack. |
| `[M-value-record-param-default-after-indirection]` | **P1 OPEN 2026-08-01 (то же окно):** value-record параметр (ABI по указателю) + дефолтный параметр ПОСЛЕ него, вызов через враппер → `indirection requires pointer operand` независимо от передачи опционального. Обход: параметр сделать обязательным. Родня C-mismatch-класса. Передано окну p-soundness-pack. |
| `[M-supervised-cancel-no-interrupt-parked-accept]` | **P1 OPEN 2026-08-01 (то же окно; РАНТАЙМ/Vela):** `supervised(cancel: tok)` вокруг `accept()` БЕЗ вложенного spawn — cancel не прерывает парковку (эмпирически: 15с таймаут, accept парковано). Со spawn работает, но перенос consume-параметра в spawn бьёт в [M-consume-param-spawn-defer-active]. Блокер graceful shutdown (222.22). M:N-окно (mn-coding-conventions), НЕ codegen. |
| `[M-consume-param-spawn-defer-active]` | **OPEN 2026-08-01 (то же окно):** перенос consume-параметра в `spawn` → `_defer_N_0_active` undeclared («Plan 217: consume-param call-arg»). Четвёртое репро в serve()-комментарии. |
| `[M-serve-router-background-tasks-not-drained]` | **✅ ЗАКРЫТ 2026-08-01 (волна p-observability): run_request дренирует take_background()/run_background (общий с servernet); rt-смок; 09-graceful /log ожил (SharedLog переведён в канон интегратором). БылоP1 OPEN 2026-08-01 (то же окно; ФУНКЦИОНАЛЬНЫЙ баг polaris):** `net/serve.nv run_request` (драйвер serve_router) не вызывает `resp.take_background()`/`tasks.drain()` — фон-задачи, поставленные через serve_router, НИКОГДА не исполняются (в отличие от servernet handle_connection). Из-за этого 09-graceful /log пуст. Чинится в polaris (маленький фикс драйвера) — волна канона наблюдаемости. |
| `[M-lsp-folder-module-peers-not-merged]` | **OPEN 2026-08-01 (скрин владельца, polaris multipart.nv):** пофайловая диагностика LSP не подтягивает peers folder-модуля — `#impl(FromRequest)` в multipart.nv красится E_IMPL_UNKNOWN_PROTOCOL, хотя FromRequest объявлен в extract.nv ТОЙ ЖЕ папки-модуля (CLI-пакет зелёный 37/0/17). Диагностический контекст файла = весь folder-module (как walk_nv: entry+peers). nova-lsp, зона 104.x. |
| `[M-lexer-niche-keywords-audit]` | **OPEN 2026-08-01 (вопрос владельца «у нас нет let & readonly»):** лексер глобально резервирует `let` (живёт только в ghost/лемма-контексте, parser:5686 expect(KwLet)) и `readonly` (L2-модификатор типа, D246) — сверить со спекой: канон ли ghost-`let` (если да — сделать контекстным словом как bench, не глобальным; если легаси — выпилить), `readonly`-аннотация — документирована ли; из подсветки оба убраны (не рекламировать ниши). Заодно `blocking/realtime/forbid/parallel/okdefer/errdefer` — тот же аудит «слово ↔ спека ↔ грамматика». Компиляторная очередь, мелочь. |
| `[M-ratelimit-buckets-unsynchronized-race]` | **✅ ЗАКРЫТ 2026-08-01 (волна p-observability): BucketTable (Mutex+HashMap, канон metrics) + конкурентный rt-смок 20 соединений. БылоP1 OPEN 2026-08-01 (вопрос владельца, middleware/ratelimit.nv @middleware):** `mut buckets = HashMap[str, TokenBucket].new()` захвачен в middleware-замыкание БЕЗ синхронизации — конкурентные insert/чтения с файберов разных соединений: гонка + UB при ресайзе HashMap (крэш под нагрузкой). Комментарий в коде сам признаёт shared-семантику. Фикс: Mutex-обёртка (образец SharedLog) — волна канона наблюдаемости polaris (после psound: Mutex.lock→ro). После энфорса mut-захватов (psound п.3) эта форма обязана краснеть компилятором — использовать как негативную фикстуру. |
| `[M-chained-consume-lock-d133-empty-type]` | **OPEN 2026-08-01 (окно p-soundness-pack п.5, побочная находка; В ICE-ПАЧКУ):** `consume g = @lock.lock()` цепочкой без явного .unlock() → ложный D133 с пустым типом — авточистка guard'а не видит тип цепочечного RHS. Из-за этого канон metrics держит явные unlock. Родня unwrapped_method_return_types-семьи. |
| `[M-supervisor-child-fail-event-lost]` | **P1 OPEN 2026-08-01 (окно p-mn-cancel, замер-проба probe173: 10×64×20):** ~40% изолированных прогонов теряют 1-2 fail-события детей — падение не доходит до decision-loop (класс видимости child_count/published, §1/§2 mn-conventions); сериализация вызова хендлера при этом ПОДТВЕРЖДЕНА (race_detected=0 везде) — гипотеза гонки опровергнута, D416§1 переформулирован. Carve-out №173 не возвращается до фикса. M:N-окно-2 (после сдачи obs/ICE; проба-гарнир — в отчёте окна p-mn-cancel). |
| `[M-os-env-get-raw-path-call-p67-ice]` | **OPEN 2026-08-01 (окно p-observability; В ICE-ПАЧКУ дозарядкой):** сырой `Os.env_get(...)` (raw-op path-call с не-`()` возвратом) валит emit_c ICE [P67-LEGACY] Path call return type unknown; обход — именованная std-обёртка env(key). Родня effect-op return-type канала. |
| `[M-operator-dispatch-checker-channel-196]` | **OPEN 2026-08-01 (честная граница волны унификации, вопрос владельца):** унификация свела 13 операторов в BINOP_TABLE/UNOP_TABLE + один резолвер (emit_c −128), но РЕШЕНИЕ «оператор→метод» всё ещё в emit-слое по C-типам операндов — не в чекере. Полный §0/196-канон: чекер резолвит операторное выражение в конкретный callee (resolved_callees, как обычный вызов), mono-регистрация — фазой, emit только исполняет; таблица остаётся единственным источником имён/арности. Отдельная 196-волна (byte-parity регресс обязателен); туда же затем ==-семья и value-record-путь ([M-operator-dispatch-unify-eq-compare-family], [M-operator-dispatch-unify-value-record-path]). |
