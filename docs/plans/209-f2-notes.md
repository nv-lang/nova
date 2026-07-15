<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 209 Ф.2 — checkpoint notes (sonnet, worktree nova-209f2 / branch plan209-f2)

База: `d5cc6679c` (main, включает Ф.1 merge). Работа ведётся ТОЛЬКО в
`d:/Sources/nv-lang/nova-209f2`; суб-агенты не спавнились.

## Задача B (HashMap-mono-gap) — статус: ГОТОВО

### Корень (найден точно, глубокий трейс кода — не по догадке)

`compiler-codegen/src/codegen/emit_c.rs`, `compute_dead_decls_with` (метод-
reachability DCE, Plan 159). Метод `T.m()` считается "живым" (тело
эмитится) ТОЛЬКО если ОБА условия верны: (a) имя типа `T` reachable
(встречается где-то в достижимом коде — для `HashMap` это ВСЕГДА так,
т.к. `Set[T]`'s type-decl `use map HashMap[T, ()]` — unconditional root),
(b) ГОЛОЕ имя метода `m` reachable (встречается как `Member`-имя в AST
где-то в достижимом коде — `collect_used_names`, чистый AST-walk).

D39 embed-delegation proxy (`emit_embed_proxies`, ~14881) — СИНТЕЗИРУЕТСЯ
в codegen НАПРЯМУЮ (никогда не существует как AST `Item::Fn`). Значит вызов
`Nova_Set_method_merge_from` → `Nova_HashMap_method_merge_from` внутри
proxy НЕВИДИМ для `collect_used_names`. Если ничто в РЕАЛЬНОМ Nova-исходнике
не пишет литерально `merge_from`/`values` (напр. `for x in some_set {}` —
итерация десугарится ВНУТРИ codegen, никогда не AST `Member`-нода) —
условие (b) не выполняется → `HashMap.merge_from`/`.values` считаются
dead → тело НЕ эмитится (но forward-decl/вызов из proxy — есть).

`static`-режим маскировал: если ничто не вызывает `Set.merge_from()`
явно — сам proxy тоже unreachable → C-компилятор (`-O2` + `-ffunction-
sections`/`--gc-sections`) целиком выкидывает proxy ДО того как линкер
увидел бы битую ссылку. Промоушн в external (Ф.1 `top_level_storage()`)
убирает эту защиту — компилятор обязан оставить external-символ
(потенциально вызываем извне), тело остаётся → линк падает на реально
не определённом `Nova_HashMap_method_merge_from`.

### Фикс (минимальный, в mono/DCE-коллекторе — как и предписано заданием)

`compute_dead_decls_with` (emit_c.rs, ~406-434): добавлен пред-скан
`module.items` собирающий `embedded_type_names` — множество имён типов,
которые КОГДА-ЛИБО embed'ятся другим record'ом (`use <field>
<EmbeddedType>[...]`, `f.is_embed`). Для метода, чей receiver-тип входит
в это множество, имя метода ПРИНУДИТЕЛЬНО кладётся в стартовый `worklist`
(reachable-by-name-alone) — ТОЧНО тот же over-keep паттерн, что уже
применяется чуть ниже в этой же функции для concrete-slice ресиверов
(`ty.starts_with("[]")`, комментарий "over-keep, never over-prune").
Консервативно (никогда не режет реально reachable код), не пытается
точно воспроизвести override-precedence логику `emit_embed_proxies` —
достаточно для устранения гейта, т.к. любой embedded-тип-метод теперь
просто ВСЕГДА эмитится (как и было бы, если б `collect_used_names` видел
codegen-синтезированный вызов).

Правка ТОЧЕЧНАЯ (~30 строк), риск низкий: затрагивает только набор
"which methods fire", никогда не убирает существующую reachability, и
не трогает механизм мангла/mono-инстанцирования generic-типов вообще.

Верификация: `cargo build --release -p nova-codegen` — 0 ошибок (только
пред-существующие warnings). Runtime-проверка (Set+link под
`NOVA_MULTI_TU=1`) — см. раздел «Совместная верификация» ниже.

## Задача A (тулчейн) — статус: ГОТОВО (Clang/Gcc), MSVC — remainder

### B1-B4 — `compiler-codegen/src/test_runner.rs`

Новая функция `pub fn compile_multi_tu_to_exe(tc, opts: &BuildOpts,
common_h: &str, parts: &[String], timeout) -> anyhow::Result<PathBuf>`
(~2067, рядом с `compile_c_to_exe`). Не трогает `build_command`/
`compile_c_to_exe` (0 изменений default-пути — `EmitOutput::Single`
по-прежнему идёт старым однокомандным путём).

- **B1** Пишет `<stem>_common.h` + `<stem>_partK.c` под `opts.obj_dir`
  (рядом — part'ы `#include` header по относительному имени).
- **B2** Компилирует каждый `part_i.c → part_i.o` **параллельно**:
  `std::thread::scope` + статичное разбиение на `min(available_parallelism,
  jobs.len())` контигентных чанков (без внешних crate-зависимостей —
  scoped threads, стабильно с 1.63). Один и тот же `flags: Vec<String>`
  (target/mode/-D/GC/-fstack.../-ffunction-sections) передаётся ВО ВСЕ
  compile-инвокейшны И в финальный линк — гарантирует ABI-инвариант
  (`-DNOVA_MAX_EFFECT_STORAGES=N` идентичен во всех TU) БЕЗ ручного
  протаскивания через N сайтов.
- **B3** Runtime `.c` (alloc/effects/fibers/fiber_arena[_win]/fiber_stats/
  runtime/driver/typeid/segv_diag + libuv net.c/fs.c/eventloop.c при
  наличии + brotli_shim.c при использовании + FFI `.c`-шимы) компилируются
  РОВНО ОДИН РАЗ (не per-part) — тот же джоб-лист, тот же тред-пул.
  Персистентный (межзапусковый) кеш `.o` НЕ реализован в этой волне —
  `-DNOVA_MAX_EFFECT_STORAGES=N` меняется от CU к CU, тривиальное
  файловое кеширование неверно давало бы stale `.o`; полноценная per-N
  корзина — Ф.3/остаток (см. «Неопределённости»).
- **B4** Линк: `flags` + все `.o` (parts + runtime) + Boehm `-lgc
  -latomic_ops` (Windows) / `-lgc [-lpthread]` (Linux) + libuv lib+syslibs
  + brotli lib(s) + FFI `-L`/`-l` — зеркалирует хвост `build_command`
  построчно.
- Ошибка любого compile-джоба или линка → `Err` с деталями (первая
  ошибка, stdout/stderr скомпилятора).

### Интеграция вызывающих (`codegen_to_c`/`run_one`, `nova-cli::cmd_build`)

- `codegen_to_c` (test_runner.rs, ~3658): переведён с `emitter.emit_module`
  на `emitter.emit_module_multi_tu(&module, &cu_name)`. Возвращает новый
  4-й элемент кортежа — `CodegenArtifact` (`Single` — `.c` записан как
  раньше побайтово; `Split{common_h, parts}` — держится В ПАМЯТИ, НЕ
  пишется рядом с `.nv`).
- `run_one` (~2955): извлекает `codegen_artifact` (borrow, до move
  `codegen_result` в error-check); `NoCFile`-проверка теперь пропускается
  для `Split`; `'cc: {}`-блок получил Split-ветку ПЕРВОЙ строкой —
  вызывает `compile_multi_tu_to_exe`, синтезирует `(CapturedOutput,
  ExitStatus)` тем же helper'ом `synth_exit_status` (через
  `ExitStatusExt::from_raw`, cfg per-OS) — ВЕСЬ последующий код
  (EXPECT_CC_ERROR match, запуск exe, EXPECT_RUNTIME_PANIC/EXIT_CODE/
  STDOUT/STDERR) остался БУКВАЛЬНО нетронут, работает для обеих веток.
  Single-ветка (retry-loop на `build_command`) — **0 изменений**, просто
  теперь под `if let CodegenArtifact::Split ... { ...; break 'cc ...}` в
  начале того же `'cc: {}`-блока, до неё код не доходит на Single-пути.
- `nova-cli/src/main.rs::cmd_build` (~4692-4942): аналогично —
  `c_code: String` → `emit_output: EmitOutput`; cache-hit ветка
  оборачивает закешированную строку в `EmitOutput::Single` (кеш остаётся
  single-`.c`-шейпед — `Split` НЕ кешируется, `build_cache::store_c`
  вызывается только если `emit_output` = `Single`); запись `.c` в tmp_path
  и финальный `compile_c_to_exe`/`compile_multi_tu_to_exe` выбираются по
  варианту `EmitOutput`.
- `nova-cli/src/bench/run.rs` — **НЕ тронут** (вне периметра задания:
  задание называло test_runner.rs/build_command/`nova-cli build`
  явно; bench — третий вызывающий, остаётся на `emit_module` напрямую,
  как и раньше — тот самый back-compat, который Ф.1 A4 гарантирует).

### Рефакторинги-помощники (низкий риск, чистые извлечения)
- `effect_count_define_arg_from_line(first_line, prefix)` — вынесен из
  `effect_count_define_arg` (тот же парсинг, теперь переиспользуется для
  `common_h`, которая уже в памяти — без диск-round-trip).
- `source_uses_brotli(src)` — вынесен из `c_file_uses_brotli` (тот же
  скан, переиспользуется по каждому in-memory part'у).

## Дефолт байт-идентичен (флаг OFF / CU ниже порога)

`emit_module_multi_tu` не трогает `emit_module` (Ф.1 гарантия, не
переисследовалась заново). Проверено здесь: `cargo build --release`
(compiler-codegen И nova-cli) — 0 ошибок, только пред-существующие
warnings. Точечная verify-таблица — см. следующий раздел (выполняется
ПОСЛЕ этого чекпоинта).

## Неопределённости / остаток

1. **MSVC toolchain** — `compile_multi_tu_to_exe` возвращает `Err` для
   `Toolchain::Msvc` (явный remainder, как и предполагал рекон §9.3).
   Вызывающие (`codegen_to_c`/`cmd_build`) СЕЙЧАС не гейтят выбор
   `emit_module_multi_tu` по toolchain'у заранее — если владелец включит
   `NOVA_MULTI_TU=1` НА MSVC-тулчейне для CU выше порога, `nova test`/
   `nova build` получит явную ошибку "not implemented for MSVC" вместо
   молчаливого fallback на single-TU. Это НАМЕРЕННО (явная ошибка лучше
   тихой деградации), но owner должен знать: **включение `NOVA_MULTI_TU`
   на Windows С MSVC-тулчейном (не clang) сейчас ломает сборку**, а не
   просто "не ускоряет". Если это неприемлемо — нужен pre-gate (проверить
   `Toolchain::Msvc` ДО вызова `emit_module_multi_tu`, форсировать `NOVA_
   MULTI_TU` off для него) — не сделано в этой волне (заказчик на этой
   машине использует Clang по умолчанию, см. `detect_toolchain`'s Windows
   preference order Clang > MSVC > GCC).
2. **Runtime-`.o` межзапусковый кеш (B5, файловый)** — НЕ реализован
   (см. B3 доку). `build_cache.rs`-интеграция (версия ключа v2→v3, набор-
   не-строка кеш) — тоже НЕ сделана (recon §7, помечен как "низкий риск,
   haiku-уровень", но требует продумывания per-CU effect-count в ключе;
   оставлен как чистый remainder Ф.3).
3. **`nova-cli/src/bench/run.rs`** — не переведён на multi-TU (вне
   периметра задания по тексту оркестратора).
4. Параллельный компайл использует статичное (contiguous chunk)
   разбиение джобов на N потоков, не work-stealing очередь — при СИЛЬНО
   неровных размерах part'ов возможен небольшой дисбаланс хвоста. Не
   мерялось на реальном 13 МБ conformance CU (вне периметра — "мега-CU
   НЕ гонять", см. задание).

## Совместная верификация (точечная, см. задание пункты 1-4)

(заполняется после прогона — см. ниже в этом же файле, секция добавлена
следующим коммитом этой же волны)
